//! Native window-layer policy for Screen Draw.
//!
//! Screen Draw intentionally uses separate z-order bands while drawing: the
//! toolbar is topmost, while the interactive canvas is at the top of the
//! normal band. Passive annotation surfaces remain topmost so that they can
//! stay visible over applications in Ghost mode, but are explicitly placed
//! immediately below the toolbar when its native handle is available.

/// Opaque native window identity used only for Screen Draw layer coordination.
///
/// The value is deliberately stored as an integer so it can cross internal
/// worker-thread boundaries without exposing general Win32 window operations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct NativeWindowHandle(isize);

impl NativeWindowHandle {
    pub(crate) const fn from_raw(raw: isize) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    #[cfg(windows)]
    fn as_hwnd(self) -> windows::Win32::Foundation::HWND {
        windows::Win32::Foundation::HWND(self.0 as *mut core::ffi::c_void)
    }
}

trait WindowLayerBackend {
    fn is_window(&self, window: NativeWindowHandle) -> bool;
    fn place_immediately_below(
        &self,
        window: NativeWindowHandle,
        ceiling: NativeWindowHandle,
    ) -> bool;
}

/// Optional z-order ceiling for topmost passive Screen Draw surfaces.
///
/// The cached handle is validated when set and again immediately before use,
/// because an egui child viewport HWND may be destroyed and recreated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ToolbarZOrderCeiling {
    window: Option<NativeWindowHandle>,
}

impl ToolbarZOrderCeiling {
    #[cfg(windows)]
    pub(crate) fn set(&mut self, window: Option<NativeWindowHandle>) {
        self.set_with(window, &WindowsWindowLayerBackend);
    }

    #[cfg(windows)]
    pub(crate) fn place_passive_below(&mut self, passive: NativeWindowHandle) -> bool {
        self.place_passive_below_with(passive, &WindowsWindowLayerBackend)
    }

    fn set_with<B: WindowLayerBackend>(&mut self, window: Option<NativeWindowHandle>, backend: &B) {
        self.window = window.filter(|window| backend.is_window(*window));
    }

    fn place_passive_below_with<B: WindowLayerBackend>(
        &mut self,
        passive: NativeWindowHandle,
        backend: &B,
    ) -> bool {
        let Some(ceiling) = self.window else {
            return false;
        };
        if !backend.is_window(ceiling) {
            self.window = None;
            return false;
        }
        backend.is_window(passive) && backend.place_immediately_below(passive, ceiling)
    }
}

#[cfg(windows)]
struct WindowsWindowLayerBackend;

#[cfg(windows)]
impl WindowLayerBackend for WindowsWindowLayerBackend {
    fn is_window(&self, window: NativeWindowHandle) -> bool {
        unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(window.as_hwnd()) }.as_bool()
    }

    fn place_immediately_below(
        &self,
        window: NativeWindowHandle,
        ceiling: NativeWindowHandle,
    ) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };

        unsafe {
            SetWindowPos(
                window.as_hwnd(),
                ceiling.as_hwnd(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        }
        .is_ok()
    }
}

#[cfg(windows)]
pub(crate) fn interactive_canvas_extended_style()
-> windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE {
    windows::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW
}

#[cfg(windows)]
pub(crate) fn interactive_canvas_style() -> windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE {
    windows::Win32::UI::WindowsAndMessaging::WS_POPUP
}

#[cfg(windows)]
pub(crate) fn passive_overlay_extended_style()
-> windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE {
    use windows::Win32::UI::WindowsAndMessaging::{
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    };

    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE
}

#[cfg(windows)]
fn interactive_canvas_position_flags()
-> windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS {
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOMOVE, SWP_NOSIZE};
    SWP_NOMOVE | SWP_NOSIZE
}

/// Raises an already-shown interactive canvas within the normal z-order band.
#[cfg(windows)]
pub(crate) fn raise_interactive_canvas(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{HWND_TOP, SetWindowPos};

    let _ = unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOP,
            0,
            0,
            0,
            0,
            interactive_canvas_position_flags(),
        )
    };
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashSet};

    use super::*;

    #[derive(Default)]
    struct FakeBackend {
        valid: RefCell<HashSet<NativeWindowHandle>>,
        placements: RefCell<Vec<(NativeWindowHandle, NativeWindowHandle)>>,
    }

    impl FakeBackend {
        fn with_valid(windows: impl IntoIterator<Item = NativeWindowHandle>) -> Self {
            Self {
                valid: RefCell::new(windows.into_iter().collect()),
                placements: RefCell::new(Vec::new()),
            }
        }
    }

    impl WindowLayerBackend for FakeBackend {
        fn is_window(&self, window: NativeWindowHandle) -> bool {
            self.valid.borrow().contains(&window)
        }

        fn place_immediately_below(
            &self,
            window: NativeWindowHandle,
            ceiling: NativeWindowHandle,
        ) -> bool {
            self.placements.borrow_mut().push((window, ceiling));
            true
        }
    }

    fn handle(raw: isize) -> NativeWindowHandle {
        NativeWindowHandle::from_raw(raw).unwrap()
    }

    #[test]
    fn rejects_null_and_invalid_toolbar_handles() {
        assert_eq!(NativeWindowHandle::from_raw(0), None);
        let backend = FakeBackend::default();
        let mut ceiling = ToolbarZOrderCeiling::default();

        ceiling.set_with(Some(handle(1)), &backend);

        assert_eq!(ceiling, ToolbarZOrderCeiling::default());
    }

    #[test]
    fn passive_surface_is_placed_immediately_below_valid_toolbar() {
        let toolbar = handle(1);
        let passive = handle(2);
        let backend = FakeBackend::with_valid([toolbar, passive]);
        let mut ceiling = ToolbarZOrderCeiling::default();
        ceiling.set_with(Some(toolbar), &backend);

        assert!(ceiling.place_passive_below_with(passive, &backend));
        assert_eq!(*backend.placements.borrow(), vec![(passive, toolbar)]);
    }

    #[test]
    fn stale_toolbar_is_cleared_before_passive_surface_is_positioned() {
        let toolbar = handle(1);
        let passive = handle(2);
        let backend = FakeBackend::with_valid([toolbar, passive]);
        let mut ceiling = ToolbarZOrderCeiling::default();
        ceiling.set_with(Some(toolbar), &backend);
        backend.valid.borrow_mut().remove(&toolbar);

        assert!(!ceiling.place_passive_below_with(passive, &backend));
        assert_eq!(ceiling, ToolbarZOrderCeiling::default());
        assert!(backend.placements.borrow().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn interactive_canvas_is_popup_toolwindow_in_normal_z_order_band() {
        use windows::Win32::UI::WindowsAndMessaging::{
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        };

        let extended = interactive_canvas_extended_style();
        assert_eq!(extended & WS_EX_TOOLWINDOW, WS_EX_TOOLWINDOW);
        assert_eq!(extended & WS_EX_TOPMOST, Default::default());
        assert_eq!(interactive_canvas_style() & WS_POPUP, WS_POPUP);

        let flags = interactive_canvas_position_flags();
        assert_eq!(flags & SWP_NOMOVE, SWP_NOMOVE);
        assert_eq!(flags & SWP_NOSIZE, SWP_NOSIZE);
        assert_eq!(flags & SWP_NOACTIVATE, Default::default());
    }

    #[cfg(windows)]
    #[test]
    fn passive_overlay_remains_topmost_layered_noactivate_and_click_through() {
        use windows::Win32::UI::WindowsAndMessaging::{
            WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        };

        let style = passive_overlay_extended_style();
        for required in [
            WS_EX_LAYERED,
            WS_EX_TRANSPARENT,
            WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST,
            WS_EX_NOACTIVATE,
        ] {
            assert_eq!(style & required, required);
        }
    }
}
