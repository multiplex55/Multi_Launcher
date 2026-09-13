//! Native window-layer policy for Screen Draw.
//!
//! Screen Draw intentionally uses separate z-order bands while drawing: the
//! toolbar is topmost, while the interactive canvas is at the top of the
//! normal band. Passive annotation surfaces remain topmost so that they can
//! stay visible over applications in Ghost mode, but are explicitly placed
//! immediately below the toolbar when its native handle is available.

use crate::screen_draw::DesktopRect;

/// Native identity and signed physical outer bounds of the Screen Draw toolbar.
///
/// The handle is absent during the bounded child-viewport resolution window;
/// bounds remain useful to the native session in that state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolbarWindowInfo {
    pub handle: Option<NativeWindowHandle>,
    pub bounds: DesktopRect,
}

/// Opaque native window identity used only for Screen Draw layer coordination.
///
/// The value is deliberately stored as an integer so it can cross internal
/// worker-thread boundaries without exposing general Win32 window operations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeWindowHandle(isize);

impl NativeWindowHandle {
    pub(crate) const fn from_raw(raw: isize) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    #[cfg(windows)]
    fn as_hwnd(self) -> windows::Win32::Foundation::HWND {
        windows::Win32::Foundation::HWND(self.0 as *mut core::ffi::c_void)
    }
}

/// Narrow native backend used by the toolbar viewport bridge.
pub(crate) trait ToolbarWindowBackend {
    fn resolve_current_thread(&self, exact_title: &str) -> Option<ToolbarWindowInfo>;
    fn inspect(&self, handle: NativeWindowHandle, exact_title: &str) -> Option<ToolbarWindowInfo>;
    fn ensure_topmost(&self, handle: NativeWindowHandle) -> bool;
}

#[derive(Debug, Default)]
pub(crate) struct SystemToolbarWindowBackend;

#[cfg(windows)]
impl ToolbarWindowBackend for SystemToolbarWindowBackend {
    fn resolve_current_thread(&self, exact_title: &str) -> Option<ToolbarWindowInfo> {
        windows_toolbar::resolve_current_thread(exact_title)
    }

    fn inspect(&self, handle: NativeWindowHandle, exact_title: &str) -> Option<ToolbarWindowInfo> {
        windows_toolbar::inspect(handle, exact_title)
    }

    fn ensure_topmost(&self, handle: NativeWindowHandle) -> bool {
        windows_toolbar::ensure_topmost(handle)
    }
}

#[cfg(not(windows))]
impl ToolbarWindowBackend for SystemToolbarWindowBackend {
    fn resolve_current_thread(&self, _exact_title: &str) -> Option<ToolbarWindowInfo> {
        None
    }

    fn inspect(
        &self,
        _handle: NativeWindowHandle,
        _exact_title: &str,
    ) -> Option<ToolbarWindowInfo> {
        None
    }

    fn ensure_topmost(&self, _handle: NativeWindowHandle) -> bool {
        false
    }
}

pub(crate) fn desktop_rect_from_native_edges(
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) -> Option<DesktopRect> {
    let width = u32::try_from(i64::from(right) - i64::from(left)).ok()?;
    let height = u32::try_from(i64::from(bottom) - i64::from(top)).ok()?;
    (width > 0 && height > 0).then(|| DesktopRect::new(left, top, width, height))
}

/// Converts the complete egui outer rectangle from points to signed physical
/// desktop pixels. Outward rounding keeps borders and draggable chrome inside
/// the temporary exclusion rectangle while the native HWND is unresolved.
pub(crate) fn desktop_rect_from_logical_edges(
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    pixels_per_point: f32,
) -> Option<DesktopRect> {
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return None;
    }
    let scaled = |value: f32, round_down: bool| -> Option<i32> {
        if !value.is_finite() {
            return None;
        }
        let value = f64::from(value) * f64::from(pixels_per_point);
        let value = if round_down {
            value.floor()
        } else {
            value.ceil()
        };
        (value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX)).then(|| value as i32)
    };
    desktop_rect_from_native_edges(
        scaled(left, true)?,
        scaled(top, true)?,
        scaled(right, false)?,
        scaled(bottom, false)?,
    )
}

pub(crate) const TOOLBAR_WINDOW_TITLE: &str = "Screen Draw — Multi Launcher";
const TOOLBAR_RESOLUTION_ATTEMPTS: u8 = 8;

/// Event-driven state machine for one egui toolbar child viewport.
///
/// Cached handles are revalidated on geometry/session lifecycle triggers, while
/// thread-window enumeration is limited to a short creation/recreation window.
/// Native updates are keyed by session generation and the complete
/// handle/bounds value.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct ScreenDrawToolbarNativeBridge {
    cached_handle: Option<NativeWindowHandle>,
    current: Option<ToolbarWindowInfo>,
    last_fallback: Option<DesktopRect>,
    last_sent: Option<(u64, Option<ToolbarWindowInfo>)>,
    resolution_attempts_left: u8,
    ensure_topmost_pending: bool,
    transition_refresh_pending: bool,
    resolution_warning_emitted: bool,
}

impl ScreenDrawToolbarNativeBridge {
    pub(crate) fn begin_viewport(&mut self) {
        self.cached_handle = None;
        self.current = None;
        self.last_fallback = None;
        self.resolution_attempts_left = TOOLBAR_RESOLUTION_ATTEMPTS;
        self.ensure_topmost_pending = true;
        self.transition_refresh_pending = true;
        self.resolution_warning_emitted = false;
    }

    /// Rearms native identity validation at an observable layer/lifecycle
    /// boundary. This is deliberately event-driven: idle frames do not inspect
    /// or enumerate windows, while an explicit Ghost/Finish/Resume transition
    /// can recover even after the initial attempt budget was exhausted.
    pub(crate) fn observe_layer_transition(&mut self) {
        self.transition_refresh_pending = true;
        self.ensure_topmost_pending = true;
        if self.cached_handle.is_none() && self.resolution_attempts_left == 0 {
            self.arm_resolution();
        }
    }

    /// Returns a nested option only when the native session needs an update:
    /// the outer option is change detection, the inner option is set/clear.
    pub(crate) fn synchronize<B: ToolbarWindowBackend>(
        &mut self,
        generation: Option<u64>,
        fallback_bounds: Option<DesktopRect>,
        backend: &B,
    ) -> Option<Option<ToolbarWindowInfo>> {
        let generation_changed = generation.is_some_and(|generation| {
            self.last_sent
                .is_none_or(|(previous, _)| previous != generation)
        });
        let geometry_changed = fallback_bounds != self.last_fallback;
        self.last_fallback = fallback_bounds;
        let inspect_cached =
            generation_changed || geometry_changed || self.transition_refresh_pending;
        self.transition_refresh_pending = false;
        self.refresh(fallback_bounds, inspect_cached, backend);
        let generation = generation?;
        let next = (generation, self.current);
        if self.last_sent == Some(next) {
            return None;
        }
        self.last_sent = Some(next);
        Some(self.current)
    }

    /// Revalidates and defensively raises the toolbar at a Resume lifecycle
    /// boundary. The caller deliberately forwards this value before Resume,
    /// even when its data is unchanged, so worker command ordering is explicit.
    pub(crate) fn synchronize_for_resume<B: ToolbarWindowBackend>(
        &mut self,
        generation: u64,
        fallback_bounds: Option<DesktopRect>,
        backend: &B,
    ) -> Option<ToolbarWindowInfo> {
        self.ensure_topmost_pending = true;
        self.transition_refresh_pending = false;
        if self.cached_handle.is_none() && self.resolution_attempts_left == 0 {
            self.arm_resolution();
        }
        self.last_fallback = fallback_bounds;
        self.refresh(fallback_bounds, true, backend);
        self.last_sent = Some((generation, self.current));
        self.current
    }

    /// Clears all viewport identity. Returns whether a previously sent native
    /// session value needs a matching clear command.
    pub(crate) fn close_viewport(&mut self) -> bool {
        let clear_needed = self.last_sent.is_some_and(|(_, toolbar)| toolbar.is_some());
        *self = Self::default();
        clear_needed
    }

    pub(crate) fn resolution_pending(&self) -> bool {
        self.cached_handle.is_none() && self.resolution_attempts_left > 0
    }

    fn arm_resolution(&mut self) {
        self.resolution_attempts_left = TOOLBAR_RESOLUTION_ATTEMPTS;
        self.ensure_topmost_pending = true;
        self.resolution_warning_emitted = false;
    }

    fn refresh<B: ToolbarWindowBackend>(
        &mut self,
        fallback_bounds: Option<DesktopRect>,
        inspect_cached: bool,
        backend: &B,
    ) {
        if let Some(handle) = self.cached_handle {
            if !inspect_cached {
                return;
            }
            if let Some(info) = backend.inspect(handle, TOOLBAR_WINDOW_TITLE) {
                if self.ensure_topmost_pending {
                    if !backend.ensure_topmost(handle) {
                        self.cached_handle = None;
                        self.current = fallback_bounds.map(|bounds| ToolbarWindowInfo {
                            handle: None,
                            bounds,
                        });
                        self.arm_resolution();
                    } else {
                        self.current = Some(info);
                        self.ensure_topmost_pending = false;
                    }
                    if self.cached_handle.is_none() {
                        // Continue below and attempt one bounded replacement
                        // resolution during this explicit refresh.
                    } else {
                        return;
                    }
                } else {
                    self.current = Some(info);
                    return;
                }
            }
            if self.cached_handle.is_some() {
                self.cached_handle = None;
                self.arm_resolution();
            }
        }

        self.current = fallback_bounds.map(|bounds| ToolbarWindowInfo {
            handle: None,
            bounds,
        });
        if self.resolution_attempts_left == 0 {
            return;
        }

        self.resolution_attempts_left -= 1;
        if let Some(info) = backend.resolve_current_thread(TOOLBAR_WINDOW_TITLE)
            && let Some(handle) = info.handle
        {
            self.cached_handle = Some(handle);
            if self.ensure_topmost_pending {
                if backend.ensure_topmost(handle) {
                    self.current = Some(info);
                    self.ensure_topmost_pending = false;
                } else {
                    self.cached_handle = None;
                    self.current = fallback_bounds.map(|bounds| ToolbarWindowInfo {
                        handle: None,
                        bounds,
                    });
                    // This candidate consumed one bounded attempt. Do not
                    // replenish the budget here or an HWND that consistently
                    // rejects SetWindowPos would create an idle repaint loop.
                }
            } else {
                self.current = Some(info);
            }
            if self.cached_handle.is_some() {
                self.resolution_warning_emitted = false;
            }
        }
        if self.cached_handle.is_none()
            && self.resolution_attempts_left == 0
            && !self.resolution_warning_emitted
        {
            tracing::warn!(
                title = TOOLBAR_WINDOW_TITLE,
                "Screen Draw toolbar HWND was not resolved during the bounded creation window"
            );
            self.resolution_warning_emitted = true;
        }
    }
}

#[cfg(windows)]
mod windows_toolbar {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, HWND_TOPMOST, IsWindow, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowPos,
    };

    use super::{NativeWindowHandle, ToolbarWindowInfo, desktop_rect_from_native_edges};

    struct ResolutionContext<'a> {
        title: &'a str,
        process_id: u32,
        thread_id: u32,
        found: Option<ToolbarWindowInfo>,
        ambiguous: bool,
    }

    pub(super) fn resolve_current_thread(exact_title: &str) -> Option<ToolbarWindowInfo> {
        let thread_id = unsafe { GetCurrentThreadId() };
        let mut context = ResolutionContext {
            title: exact_title,
            process_id: unsafe { GetCurrentProcessId() },
            thread_id,
            found: None,
            ambiguous: false,
        };
        unsafe {
            let _ = EnumThreadWindows(
                thread_id,
                Some(resolve_callback),
                LPARAM((&mut context as *mut ResolutionContext<'_>) as isize),
            );
        }
        (!context.ambiguous).then_some(context.found).flatten()
    }

    pub(super) fn inspect(
        handle: NativeWindowHandle,
        exact_title: &str,
    ) -> Option<ToolbarWindowInfo> {
        inspect_hwnd(
            handle.as_hwnd(),
            exact_title,
            unsafe { GetCurrentProcessId() },
            unsafe { GetCurrentThreadId() },
        )
    }

    pub(super) fn ensure_topmost(handle: NativeWindowHandle) -> bool {
        // Revalidate the complete identity immediately before the mutating
        // z-order call, rather than trusting an earlier IsWindow result for an
        // ephemeral HWND that Windows could already have recycled.
        if inspect(handle, super::TOOLBAR_WINDOW_TITLE).is_none() {
            return false;
        }
        unsafe {
            SetWindowPos(
                handle.as_hwnd(),
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        }
        .is_ok()
    }

    unsafe extern "system" fn resolve_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let context = unsafe { &mut *(lparam.0 as *mut ResolutionContext<'_>) };
        if let Some(info) = inspect_hwnd(hwnd, context.title, context.process_id, context.thread_id)
        {
            if context.found.replace(info).is_some() {
                context.ambiguous = true;
            }
        }
        BOOL(1)
    }

    fn inspect_hwnd(
        hwnd: HWND,
        exact_title: &str,
        expected_process_id: u32,
        expected_thread_id: u32,
    ) -> Option<ToolbarWindowInfo> {
        if !unsafe { IsWindow(hwnd) }.as_bool() {
            return None;
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id != expected_process_id || thread_id != expected_thread_id {
            return None;
        }
        let title_len = usize::try_from(unsafe { GetWindowTextLengthW(hwnd) }).ok()?;
        let mut title = vec![0_u16; title_len.checked_add(1)?];
        let copied = usize::try_from(unsafe { GetWindowTextW(hwnd, &mut title) }).ok()?;
        if String::from_utf16_lossy(&title[..copied]) != exact_title {
            return None;
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
        let bounds = desktop_rect_from_native_edges(rect.left, rect.top, rect.right, rect.bottom)?;
        Some(ToolbarWindowInfo {
            handle: NativeWindowHandle::from_raw(hwnd.0 as isize),
            bounds,
        })
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
    use std::{
        cell::RefCell,
        collections::{HashMap, HashSet, VecDeque},
    };

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

    #[derive(Default)]
    struct FakeToolbarBackend {
        resolutions: RefCell<VecDeque<Option<ToolbarWindowInfo>>>,
        inspected: RefCell<HashMap<NativeWindowHandle, ToolbarWindowInfo>>,
        resolve_calls: RefCell<Vec<String>>,
        inspect_calls: RefCell<Vec<(NativeWindowHandle, String)>>,
        topmost_calls: RefCell<Vec<NativeWindowHandle>>,
        topmost_results: RefCell<VecDeque<bool>>,
    }

    impl FakeToolbarBackend {
        fn queue_resolution(&self, info: Option<ToolbarWindowInfo>) {
            self.resolutions.borrow_mut().push_back(info);
        }

        fn set_inspected(&self, info: ToolbarWindowInfo) {
            self.inspected
                .borrow_mut()
                .insert(info.handle.unwrap(), info);
        }

        fn queue_topmost_result(&self, result: bool) {
            self.topmost_results.borrow_mut().push_back(result);
        }
    }

    impl ToolbarWindowBackend for FakeToolbarBackend {
        fn resolve_current_thread(&self, exact_title: &str) -> Option<ToolbarWindowInfo> {
            self.resolve_calls.borrow_mut().push(exact_title.to_owned());
            let info = self.resolutions.borrow_mut().pop_front().flatten();
            if let Some(info) = info {
                self.set_inspected(info);
            }
            info
        }

        fn inspect(
            &self,
            handle: NativeWindowHandle,
            exact_title: &str,
        ) -> Option<ToolbarWindowInfo> {
            self.inspect_calls
                .borrow_mut()
                .push((handle, exact_title.to_owned()));
            self.inspected.borrow().get(&handle).copied()
        }

        fn ensure_topmost(&self, handle: NativeWindowHandle) -> bool {
            self.topmost_calls.borrow_mut().push(handle);
            self.topmost_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(true)
        }
    }

    fn toolbar_info(raw: isize, bounds: DesktopRect) -> ToolbarWindowInfo {
        ToolbarWindowInfo {
            handle: Some(handle(raw)),
            bounds,
        }
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

    #[test]
    fn native_rect_and_logical_fallback_conversion_are_checked_signed_and_dpi_aware() {
        assert_eq!(
            desktop_rect_from_native_edges(-1800, -200, -1400, 500),
            Some(DesktopRect::new(-1800, -200, 400, 700))
        );
        assert_eq!(desktop_rect_from_native_edges(20, 10, 20, 40), None);
        assert_eq!(desktop_rect_from_native_edges(20, 10, 19, 40), None);
        assert_eq!(
            desktop_rect_from_logical_edges(-80.0, 25.0, 220.0, 725.0, 1.0),
            Some(DesktopRect::new(-80, 25, 300, 700))
        );

        assert_eq!(
            desktop_rect_from_logical_edges(-100.25, -20.5, 99.25, 79.5, 1.25),
            Some(DesktopRect::new(-126, -26, 251, 126))
        );
        assert_eq!(
            desktop_rect_from_logical_edges(-1200.0, 100.0, -920.0, 800.0, 1.5),
            Some(DesktopRect::new(-1800, 150, 420, 1050))
        );
        assert!(desktop_rect_from_logical_edges(0.0, 0.0, 10.0, 10.0, f32::NAN).is_none());
    }

    #[test]
    fn bridge_resolves_once_revalidates_cache_and_sends_only_real_changes() {
        let backend = FakeToolbarBackend::default();
        let fallback = DesktopRect::new(-400, 20, 300, 700);
        let native = toolbar_info(11, DesktopRect::new(-405, 15, 310, 710));
        backend.queue_resolution(Some(native));
        let mut bridge = ScreenDrawToolbarNativeBridge::default();
        bridge.begin_viewport();

        assert_eq!(
            bridge.synchronize(Some(7), Some(fallback), &backend),
            Some(Some(native))
        );
        for _ in 0..5 {
            assert_eq!(bridge.synchronize(Some(7), Some(fallback), &backend), None);
        }
        assert_eq!(backend.resolve_calls.borrow().len(), 1);
        assert_eq!(*backend.topmost_calls.borrow(), vec![handle(11)]);
        assert!(backend.inspect_calls.borrow().is_empty());

        let moved = toolbar_info(11, DesktopRect::new(-205, 15, 310, 710));
        backend.set_inspected(moved);
        let moved_fallback = DesktopRect::new(-200, 20, 300, 700);
        assert_eq!(
            bridge.synchronize(Some(7), Some(moved_fallback), &backend),
            Some(Some(moved))
        );
        assert_eq!(backend.resolve_calls.borrow().len(), 1);
        assert_eq!(backend.topmost_calls.borrow().len(), 1);
        assert_eq!(backend.inspect_calls.borrow().len(), 1);

        // A replacement native session receives the current identity even
        // when the viewport itself did not change.
        assert_eq!(
            bridge.synchronize(Some(8), Some(moved_fallback), &backend),
            Some(Some(moved))
        );
        assert_eq!(backend.inspect_calls.borrow().len(), 2);
    }

    #[test]
    fn stale_handle_rearms_bounded_resolution_and_recreation_gets_fresh_identity() {
        let backend = FakeToolbarBackend::default();
        let first = toolbar_info(21, DesktopRect::new(10, 20, 300, 700));
        let second = toolbar_info(22, DesktopRect::new(-900, -50, 450, 900));
        backend.queue_resolution(Some(first));
        let mut bridge = ScreenDrawToolbarNativeBridge::default();
        bridge.begin_viewport();
        assert_eq!(
            bridge.synchronize(Some(1), Some(first.bounds), &backend),
            Some(Some(first))
        );

        backend.inspected.borrow_mut().remove(&handle(21));
        backend.queue_resolution(Some(second));
        assert_eq!(
            bridge.synchronize(Some(1), Some(second.bounds), &backend),
            Some(Some(second))
        );
        assert_eq!(
            *backend.topmost_calls.borrow(),
            vec![handle(21), handle(22)]
        );
        assert_eq!(backend.resolve_calls.borrow().len(), 2);
    }

    #[test]
    fn unresolved_bridge_uses_full_fallback_and_never_enumerates_on_move() {
        let backend = FakeToolbarBackend::default();
        let fallback = DesktopRect::new(-100, -200, 330, 740);
        let mut bridge = ScreenDrawToolbarNativeBridge::default();
        bridge.begin_viewport();

        assert_eq!(
            bridge.synchronize(Some(3), Some(fallback), &backend),
            Some(Some(ToolbarWindowInfo {
                handle: None,
                bounds: fallback,
            }))
        );
        for _ in 0..(TOOLBAR_RESOLUTION_ATTEMPTS + 3) {
            assert_eq!(bridge.synchronize(Some(3), Some(fallback), &backend), None);
        }
        assert_eq!(
            backend.resolve_calls.borrow().len(),
            usize::from(TOOLBAR_RESOLUTION_ATTEMPTS)
        );

        let moved = DesktopRect::new(-50, -200, 330, 740);
        assert_eq!(
            bridge.synchronize(Some(3), Some(moved), &backend),
            Some(Some(ToolbarWindowInfo {
                handle: None,
                bounds: moved,
            }))
        );
        assert_eq!(
            backend.resolve_calls.borrow().len(),
            usize::from(TOOLBAR_RESOLUTION_ATTEMPTS)
        );
    }

    #[test]
    fn close_clears_sent_identity_and_resume_revalidates_and_raises_once() {
        let backend = FakeToolbarBackend::default();
        let info = toolbar_info(31, DesktopRect::new(100, 200, 320, 720));
        backend.queue_resolution(Some(info));
        let mut bridge = ScreenDrawToolbarNativeBridge::default();
        bridge.begin_viewport();
        assert!(
            bridge
                .synchronize(Some(5), Some(info.bounds), &backend)
                .is_some()
        );

        assert_eq!(
            bridge.synchronize_for_resume(5, Some(info.bounds), &backend),
            Some(info)
        );
        assert_eq!(backend.inspect_calls.borrow().len(), 1);
        assert_eq!(
            *backend.topmost_calls.borrow(),
            vec![handle(31), handle(31)]
        );
        assert!(bridge.close_viewport());
        assert!(!bridge.close_viewport());
        assert_eq!(bridge, ScreenDrawToolbarNativeBridge::default());
    }

    #[test]
    fn explicit_transition_recovers_after_attempt_budget_exhaustion_without_idle_polling() {
        let backend = FakeToolbarBackend::default();
        let fallback = DesktopRect::new(20, 30, 300, 700);
        let recovered = toolbar_info(41, DesktopRect::new(18, 28, 304, 704));
        let mut bridge = ScreenDrawToolbarNativeBridge::default();
        bridge.begin_viewport();
        for _ in 0..TOOLBAR_RESOLUTION_ATTEMPTS {
            let _ = bridge.synchronize(Some(9), Some(fallback), &backend);
        }
        let exhausted_calls = backend.resolve_calls.borrow().len();
        assert_eq!(exhausted_calls, usize::from(TOOLBAR_RESOLUTION_ATTEMPTS));
        assert_eq!(bridge.synchronize(Some(9), Some(fallback), &backend), None);
        assert_eq!(backend.resolve_calls.borrow().len(), exhausted_calls);

        backend.queue_resolution(Some(recovered));
        bridge.observe_layer_transition();
        assert_eq!(
            bridge.synchronize(Some(9), Some(fallback), &backend),
            Some(Some(recovered))
        );
        assert_eq!(backend.resolve_calls.borrow().len(), exhausted_calls + 1);
    }

    #[test]
    fn persistent_topmost_failure_exhausts_exact_budget_and_then_stays_idle() {
        let backend = FakeToolbarBackend::default();
        let fallback = DesktopRect::new(-20, 30, 300, 700);
        for raw in 70..70 + isize::from(TOOLBAR_RESOLUTION_ATTEMPTS) {
            backend.queue_resolution(Some(toolbar_info(raw, fallback)));
            backend.queue_topmost_result(false);
        }
        let mut bridge = ScreenDrawToolbarNativeBridge::default();
        bridge.begin_viewport();

        for _ in 0..TOOLBAR_RESOLUTION_ATTEMPTS {
            let _ = bridge.synchronize(Some(15), Some(fallback), &backend);
        }
        assert!(!bridge.resolution_pending());
        assert!(bridge.resolution_warning_emitted);
        assert_eq!(
            backend.resolve_calls.borrow().len(),
            usize::from(TOOLBAR_RESOLUTION_ATTEMPTS)
        );
        assert_eq!(
            backend.topmost_calls.borrow().len(),
            usize::from(TOOLBAR_RESOLUTION_ATTEMPTS)
        );

        for _ in 0..5 {
            assert_eq!(bridge.synchronize(Some(15), Some(fallback), &backend), None);
        }
        assert_eq!(
            backend.resolve_calls.borrow().len(),
            usize::from(TOOLBAR_RESOLUTION_ATTEMPTS)
        );
        assert_eq!(
            backend.topmost_calls.borrow().len(),
            usize::from(TOOLBAR_RESOLUTION_ATTEMPTS)
        );
    }

    #[test]
    fn topmost_failure_marks_same_bounds_handle_stale_and_uses_replacement() {
        let backend = FakeToolbarBackend::default();
        let bounds = DesktopRect::new(100, 100, 300, 700);
        let stale = toolbar_info(51, bounds);
        let replacement = toolbar_info(52, bounds);
        backend.queue_resolution(Some(stale));
        let mut bridge = ScreenDrawToolbarNativeBridge::default();
        bridge.begin_viewport();
        assert_eq!(
            bridge.synchronize(Some(4), Some(bounds), &backend),
            Some(Some(stale))
        );

        backend.queue_topmost_result(false);
        backend.queue_resolution(Some(replacement));
        bridge.observe_layer_transition();
        assert_eq!(
            bridge.synchronize(Some(4), Some(bounds), &backend),
            Some(Some(replacement))
        );
        assert_eq!(
            *backend.topmost_calls.borrow(),
            vec![handle(51), handle(51), handle(52)]
        );
    }

    #[test]
    fn explicit_transition_recovers_stale_handle_without_generation_or_bounds_change() {
        let backend = FakeToolbarBackend::default();
        let bounds = DesktopRect::new(-300, 50, 300, 700);
        let stale = toolbar_info(61, bounds);
        let replacement = toolbar_info(62, bounds);
        backend.queue_resolution(Some(stale));
        let mut bridge = ScreenDrawToolbarNativeBridge::default();
        bridge.begin_viewport();
        assert_eq!(
            bridge.synchronize(Some(12), Some(bounds), &backend),
            Some(Some(stale))
        );

        backend.inspected.borrow_mut().remove(&handle(61));
        backend.queue_resolution(Some(replacement));
        bridge.observe_layer_transition();
        assert_eq!(
            bridge.synchronize(Some(12), Some(bounds), &backend),
            Some(Some(replacement))
        );
        assert_eq!(backend.inspect_calls.borrow().len(), 1);
        assert_eq!(backend.resolve_calls.borrow().len(), 2);
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
