//! Checked, event-driven Win32 move/resize observation shared by GUI hosts.
//!
//! The owner keeps the bounded completion slot after the HWND binding retires,
//! so a completed `WM_EXITSIZEMOVE` remains readable through `WM_NCDESTROY`.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use crate::settings::{NativeWindowGeometry, PreferredWindowSize, UtilityToolKey};

/// Physical outer-window bounds in virtual-desktop coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl NativeWindowRect {
    pub fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    pub fn is_valid(self) -> bool {
        self.width() > 0 && self.height() > 0
    }

    pub(crate) fn matches(self, other: Self) -> bool {
        self.left.abs_diff(other.left) <= 2
            && self.top.abs_diff(other.top) <= 2
            && self.right.abs_diff(other.right) <= 2
            && self.bottom.abs_diff(other.bottom) <= 2
    }
}

/// Fences a completed native gesture to one host mount and domain session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeGeometryStamp {
    pub session_epoch: u64,
    pub mount_generation: u64,
}

/// One completed native move/resize gesture. Position and size ordering are
/// independent so a later work-area clamp cannot erase an explicit resize.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeGeometryGesture {
    pub tool_key: UtilityToolKey,
    pub stamp: NativeGeometryStamp,
    pub position: NativeWindowRect,
    pub position_scale: f32,
    pub position_sequence: u64,
    pub size: Option<NativeWindowRect>,
    pub size_sequence: Option<u64>,
}

pub type NativeGeometrySlot = Arc<Mutex<Option<NativeGeometryGesture>>>;

pub(crate) fn root_is_live(hwnd: usize) -> bool {
    #[cfg(windows)]
    {
        native::root_is_live(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

pub fn new_native_geometry_slot() -> NativeGeometrySlot {
    Arc::new(Mutex::new(None))
}

/// Take the completion only when it belongs to the expected live session and
/// mount. A mismatched sample is retired with the stale host.
pub fn take_native_geometry(
    slot: &NativeGeometrySlot,
    tool_key: UtilityToolKey,
    stamp: NativeGeometryStamp,
) -> Option<NativeGeometryGesture> {
    let mut slot = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    slot.take()
        .filter(|sample| sample.tool_key == tool_key && sample.stamp == stamp)
}

fn merge_gesture(
    current: Option<NativeGeometryGesture>,
    next: NativeGeometryGesture,
) -> NativeGeometryGesture {
    let Some(current) =
        current.filter(|current| current.tool_key == next.tool_key && current.stamp == next.stamp)
    else {
        return next;
    };
    let position = if next.position_sequence > current.position_sequence {
        next
    } else {
        current
    };
    let size = if next.size_sequence > current.size_sequence {
        next
    } else {
        current
    };
    NativeGeometryGesture {
        tool_key: position.tool_key,
        stamp: position.stamp,
        position: position.position,
        position_scale: position.position_scale,
        position_sequence: position.position_sequence,
        size: size.size,
        size_sequence: size.size_sequence,
    }
}

/// A checked HWND identity captured from the owning GPUI/egui window callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckedNativeWindow {
    pub hwnd: usize,
    pub process_id: u32,
    pub thread_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeWindowMetrics {
    pub outer: NativeWindowRect,
    pub client_width: i32,
    pub client_height: i32,
    pub scale_factor: f32,
}

/// Read checked physical dimensions and the actual destination window DPI.
pub fn native_window_metrics(
    target: CheckedNativeWindow,
    zoom_factor: f32,
) -> Result<Option<NativeWindowMetrics>, String> {
    #[cfg(windows)]
    {
        native::window_metrics(target, zoom_factor).map(Some)
    }
    #[cfg(not(windows))]
    {
        let _ = (target, zoom_factor);
        Ok(None)
    }
}

/// Restore saved physical outer geometry on the HWND's owning UI thread. The
/// first placement selects the destination monitor; only then are its actual
/// DPI and window chrome measured for the final work-area-constrained rect.
pub fn restore_native_window_geometry(
    target: CheckedNativeWindow,
    geometry: Option<NativeWindowGeometry>,
    minimum_client: (i32, i32),
    zoom_factor: f32,
) -> Result<Option<NativeWindowMetrics>, String> {
    #[cfg(windows)]
    {
        let Some(geometry) = geometry else {
            return native::window_metrics(target, zoom_factor).map(Some);
        };
        native::restore_geometry(target, geometry, minimum_client, zoom_factor).map(Some)
    }
    #[cfg(not(windows))]
    {
        let _ = (target, geometry, minimum_client, zoom_factor);
        Ok(None)
    }
}

/// Pure geometry clamp used by the native restore path and its unit tests.
pub fn clamp_outer_rect_to_work_area(
    desired: NativeWindowRect,
    work_area: NativeWindowRect,
    minimum_outer: (i32, i32),
) -> Option<NativeWindowRect> {
    let work_width = work_area.width();
    let work_height = work_area.height();
    if work_width <= 0 || work_height <= 0 || !desired.is_valid() {
        return None;
    }
    let width = desired.width().max(minimum_outer.0.max(1)).min(work_width);
    let height = desired
        .height()
        .max(minimum_outer.1.max(1))
        .min(work_height);
    let max_left = work_area.right.saturating_sub(width);
    let max_top = work_area.bottom.saturating_sub(height);
    let left = desired.left.clamp(work_area.left, max_left);
    let top = desired.top.clamp(work_area.top, max_top);
    Some(NativeWindowRect {
        left,
        top,
        right: left.saturating_add(width),
        bottom: top.saturating_add(height),
    })
}

pub struct NativeGeometryBinding {
    #[cfg(windows)]
    inner: Option<native::SubclassBinding>,
}

impl NativeGeometryBinding {
    /// Attach to an HWND already identified by its owning frontend. The HWND
    /// must still belong to this process and the current UI thread.
    pub fn attach_checked(
        target: CheckedNativeWindow,
        tool_key: UtilityToolKey,
        stamp: NativeGeometryStamp,
        slot: NativeGeometrySlot,
        zoom_factor: f32,
        sequence: Arc<AtomicU64>,
        wake: Arc<dyn Fn(usize) + Send + Sync>,
    ) -> Result<Self, String> {
        #[cfg(windows)]
        {
            let binding =
                native::attach_checked(target, tool_key, stamp, slot, zoom_factor, sequence, wake)?;
            Ok(Self {
                inner: Some(binding),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (target, tool_key, stamp, slot, zoom_factor, sequence, wake);
            Ok(Self {})
        }
    }

    /// Attach to an egui utility window after the adapter has uniquely matched
    /// its title, owner, process, UI thread and expected outer bounds.
    pub(crate) fn attach_discovered(
        title: &str,
        root_hwnd: usize,
        expected_bounds: NativeWindowRect,
        tool_key: UtilityToolKey,
        stamp: NativeGeometryStamp,
        slot: NativeGeometrySlot,
        zoom_factor: f32,
        sequence: Arc<AtomicU64>,
        wake: Arc<dyn Fn(usize) + Send + Sync>,
    ) -> Result<Option<Self>, String> {
        #[cfg(windows)]
        {
            let Some(target) = native::find_child(title, root_hwnd, expected_bounds)? else {
                return Ok(None);
            };
            let binding =
                native::attach_checked(target, tool_key, stamp, slot, zoom_factor, sequence, wake)?;
            Ok(Some(Self {
                inner: Some(binding),
            }))
        }
        #[cfg(not(windows))]
        {
            let _ = (
                title,
                root_hwnd,
                expected_bounds,
                tool_key,
                stamp,
                slot,
                zoom_factor,
                sequence,
                wake,
            );
            Ok(None)
        }
    }

    pub fn is_live(&self) -> bool {
        #[cfg(windows)]
        {
            self.inner.as_ref().is_some_and(native::binding_is_live)
        }
        #[cfg(not(windows))]
        {
            true
        }
    }

    pub(crate) fn window_handle(&self) -> Option<usize> {
        #[cfg(windows)]
        {
            self.inner.as_ref().and_then(native::window_handle)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub(crate) fn update_zoom(&self, zoom_factor: f32) {
        #[cfg(windows)]
        if let Some(inner) = self.inner.as_ref() {
            native::update_zoom(inner, zoom_factor);
        }
        #[cfg(not(windows))]
        let _ = zoom_factor;
    }
}

impl Drop for NativeGeometryBinding {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(binding) = self.inner.take() {
            native::detach(binding);
        }
    }
}

#[cfg(windows)]
mod native {
    use super::{
        CheckedNativeWindow, NativeGeometryGesture, NativeGeometrySlot, NativeGeometryStamp,
        NativeWindowGeometry, NativeWindowMetrics, NativeWindowRect, clamp_outer_rect_to_work_area,
        merge_gesture,
    };
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
    };
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GA_ROOT, GA_ROOTOWNER, GetAncestor, GetClientRect, GetPropW, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
        IsZoomed, RemovePropW, SWP_NOACTIVATE, SWP_NOZORDER, SetPropW, SetWindowPos,
        WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVING, WM_NCDESTROY, WM_SIZING,
    };

    const SUBCLASS_ID: usize = 0x4D4C_5554;
    const BINDING_PROPERTY_NAME: windows::core::PCWSTR =
        windows::core::w!("MultiLauncher.UtilityGeometry.Binding");

    #[derive(Clone, Copy)]
    struct GestureStart {
        rect: NativeWindowRect,
        scale: f32,
        resized: bool,
    }

    struct SubclassData {
        tool_key: crate::settings::UtilityToolKey,
        stamp: NativeGeometryStamp,
        slot: NativeGeometrySlot,
        sequence: Arc<AtomicU64>,
        gesture_start: Mutex<Option<GestureStart>>,
        zoom_factor_bits: AtomicU32,
        raw_reference_released: AtomicBool,
        wake: Arc<dyn Fn(usize) + Send + Sync>,
    }

    pub(super) struct SubclassBinding {
        hwnd: usize,
        raw_data: usize,
        data: Arc<SubclassData>,
    }

    struct Search<'a> {
        title: &'a [u16],
        root: HWND,
        process_id: u32,
        thread_id: u32,
        expected_bounds: NativeWindowRect,
        matches: Vec<HWND>,
    }

    pub(super) fn root_is_live(hwnd: usize) -> bool {
        let root = HWND(hwnd as *mut c_void);
        let mut process_id = 0;
        hwnd != 0
            && unsafe { IsWindow(root) }.as_bool()
            && unsafe { GetAncestor(root, GA_ROOT) } == root
            && unsafe { GetWindowThreadProcessId(root, Some(&mut process_id)) } != 0
            && process_id == std::process::id()
    }

    unsafe extern "system" fn find_exact_child(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let search = unsafe { &mut *(parameter.0 as *mut Search<'_>) };
        if hwnd == search.root || !unsafe { IsWindow(hwnd) }.as_bool() {
            return BOOL(1);
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        let owner = unsafe { GetAncestor(hwnd, GA_ROOTOWNER) };
        if process_id != search.process_id
            || thread_id != search.thread_id
            || (owner != search.root && owner != hwnd)
        {
            return BOOL(1);
        }
        let title_length = unsafe { GetWindowTextLengthW(hwnd) };
        if title_length <= 0 {
            return BOOL(1);
        }
        let mut title = vec![0_u16; title_length as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut title) };
        if copied > 0
            && &title[..copied as usize] == search.title
            && window_bounds(hwnd).is_some_and(|bounds| bounds.matches(search.expected_bounds))
        {
            search.matches.push(hwnd);
        }
        BOOL(1)
    }

    pub(super) fn find_child(
        title: &str,
        root_hwnd: usize,
        expected_bounds: NativeWindowRect,
    ) -> Result<Option<CheckedNativeWindow>, String> {
        let root = HWND(root_hwnd as *mut c_void);
        if root_hwnd == 0
            || !unsafe { IsWindow(root) }.as_bool()
            || unsafe { GetAncestor(root, GA_ROOT) } != root
        {
            return Err("the current root HWND is not a live top-level window".into());
        }
        let mut process_id = 0;
        let root_thread = unsafe { GetWindowThreadProcessId(root, Some(&mut process_id)) };
        if process_id != std::process::id() {
            return Err("the root HWND belongs to another process".into());
        }
        let title_wide = title.encode_utf16().collect::<Vec<_>>();
        let mut search = Search {
            title: &title_wide,
            root,
            process_id,
            thread_id: root_thread,
            expected_bounds,
            matches: Vec::new(),
        };
        unsafe {
            EnumWindows(
                Some(find_exact_child),
                LPARAM((&mut search as *mut Search<'_>) as isize),
            )
        }
        .map_err(|error| format!("could not enumerate utility viewports: {error}"))?;
        match search.matches.as_slice() {
            [] => Ok(None),
            [hwnd] => {
                let mut process_id = 0;
                let thread_id = unsafe { GetWindowThreadProcessId(*hwnd, Some(&mut process_id)) };
                Ok(Some(CheckedNativeWindow {
                    hwnd: hwnd.0 as usize,
                    process_id,
                    thread_id,
                }))
            }
            _ => Err("multiple native child windows matched one utility viewport".into()),
        }
    }

    fn rect(hwnd: HWND) -> Option<NativeWindowRect> {
        if unsafe { !IsWindow(hwnd).as_bool() }
            || unsafe { IsIconic(hwnd) }.as_bool()
            || unsafe { IsZoomed(hwnd) }.as_bool()
        {
            return None;
        }
        window_bounds(hwnd)
    }

    fn window_bounds(hwnd: HWND) -> Option<NativeWindowRect> {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
        let rect = NativeWindowRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        rect.is_valid().then_some(rect)
    }

    fn checked_hwnd(target: CheckedNativeWindow) -> Result<HWND, String> {
        let hwnd = HWND(target.hwnd as *mut c_void);
        if target.hwnd == 0 || !unsafe { IsWindow(hwnd) }.as_bool() {
            return Err("the utility window HWND is no longer live".into());
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        let current_thread = unsafe { GetCurrentThreadId() };
        if process_id != target.process_id
            || process_id != std::process::id()
            || thread_id != target.thread_id
            || thread_id != current_thread
        {
            return Err("the utility HWND process or owning UI thread changed".into());
        }
        Ok(hwnd)
    }

    pub(super) fn window_metrics(
        target: CheckedNativeWindow,
        zoom_factor: f32,
    ) -> Result<NativeWindowMetrics, String> {
        let hwnd = checked_hwnd(target)?;
        let outer = window_bounds(hwnd)
            .ok_or_else(|| "the utility HWND has no normal outer rectangle".to_owned())?;
        let mut client = RECT::default();
        unsafe { GetClientRect(hwnd, &mut client) }
            .map_err(|error| format!("could not read the utility client bounds: {error}"))?;
        let client_width = client.right.saturating_sub(client.left);
        let client_height = client.bottom.saturating_sub(client.top);
        let scale_factor = scale(hwnd, zoom_factor)
            .ok_or_else(|| "could not read a valid destination window DPI".to_owned())?;
        Ok(NativeWindowMetrics {
            outer,
            client_width,
            client_height,
            scale_factor,
        })
    }

    fn nearest_work_area(rect: NativeWindowRect) -> Result<NativeWindowRect, String> {
        let rect = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        let monitor = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST) };
        if monitor.0.is_null() {
            return Err("Windows could not identify the target monitor".to_owned());
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return Err("Windows could not read the target monitor work area".to_owned());
        }
        let work = info.rcWork;
        let work = NativeWindowRect {
            left: work.left,
            top: work.top,
            right: work.right,
            bottom: work.bottom,
        };
        if work.is_valid() {
            Ok(work)
        } else {
            Err("the target monitor has an invalid work area".to_owned())
        }
    }

    fn set_outer_rect(hwnd: HWND, rect: NativeWindowRect) -> Result<(), String> {
        unsafe {
            SetWindowPos(
                hwnd,
                HWND::default(),
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        }
        .map_err(|error| format!("Windows rejected the Hash Generator placement: {error}"))
    }

    fn scaled_dimension(value: f32, scale: f32) -> Result<i32, String> {
        let physical = value as f64 * scale as f64;
        if !physical.is_finite() || physical <= 0.0 || physical > i32::MAX as f64 {
            return Err("saved Hash Generator geometry is outside the native desktop range".into());
        }
        Ok(physical.round() as i32)
    }

    fn scaled_coordinate(value: f32, scale: f32) -> Result<i32, String> {
        let physical = value as f64 * scale as f64;
        if !physical.is_finite() || physical < i32::MIN as f64 || physical > i32::MAX as f64 {
            return Err("saved Hash Generator position is outside the native desktop range".into());
        }
        Ok(physical.round() as i32)
    }

    pub(super) fn restore_geometry(
        target: CheckedNativeWindow,
        geometry: NativeWindowGeometry,
        minimum_client: (i32, i32),
        zoom_factor: f32,
    ) -> Result<NativeWindowMetrics, String> {
        let hwnd = checked_hwnd(target)?;
        let desired = NativeWindowRect {
            left: scaled_coordinate(geometry.position.x, geometry.scale_factor)?,
            top: scaled_coordinate(geometry.position.y, geometry.scale_factor)?,
            right: 0,
            bottom: 0,
        };
        let width = scaled_dimension(geometry.size.width, geometry.scale_factor)?;
        let height = scaled_dimension(geometry.size.height, geometry.scale_factor)?;
        let desired = NativeWindowRect {
            right: desired.left.saturating_add(width),
            bottom: desired.top.saturating_add(height),
            ..desired
        };

        // Move to the saved target monitor first. Windows may synchronously
        // dispatch WM_DPICHANGED and adjust the frame during this placement.
        let first_work_area = nearest_work_area(desired)?;
        let first_rect = clamp_outer_rect_to_work_area(desired, first_work_area, (1, 1))
            .ok_or_else(|| "could not clamp the saved Hash Generator rectangle".to_owned())?;
        set_outer_rect(hwnd, first_rect)?;

        let measured = window_metrics(target, zoom_factor)?;
        let work_area = nearest_work_area(measured.outer)?;
        let chrome_width = measured
            .outer
            .width()
            .saturating_sub(measured.client_width)
            .max(0);
        let chrome_height = measured
            .outer
            .height()
            .saturating_sub(measured.client_height)
            .max(0);
        let minimum_outer = (
            scaled_dimension(minimum_client.0 as f32, measured.scale_factor)?
                .saturating_add(chrome_width),
            scaled_dimension(minimum_client.1 as f32, measured.scale_factor)?
                .saturating_add(chrome_height),
        );
        let target_rect = NativeWindowRect {
            left: desired.left,
            top: desired.top,
            right: desired.left.saturating_add(width),
            bottom: desired.top.saturating_add(height),
        };
        let final_rect = clamp_outer_rect_to_work_area(target_rect, work_area, minimum_outer)
            .ok_or_else(|| "could not clamp the destination Hash Generator rectangle".to_owned())?;
        if final_rect != measured.outer {
            set_outer_rect(hwnd, final_rect)?;
        }
        window_metrics(target, zoom_factor)
    }

    fn scale(hwnd: HWND, zoom_factor: f32) -> Option<f32> {
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        if dpi == 0 || !zoom_factor.is_finite() || zoom_factor <= 0.0 {
            return None;
        }
        let scale = dpi as f32 / 96.0 * zoom_factor;
        (scale.is_finite() && scale > 0.0).then_some(scale)
    }

    unsafe extern "system" fn utility_subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        reference_data: usize,
    ) -> LRESULT {
        let data = unsafe { &*(reference_data as *const SubclassData) };
        match message {
            WM_ENTERSIZEMOVE => {
                let start = rect(hwnd).and_then(|rect| {
                    scale(hwnd, data.zoom_factor()).map(|scale| GestureStart {
                        rect,
                        scale,
                        resized: false,
                    })
                });
                *data
                    .gesture_start
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = start;
            }
            WM_SIZING => {
                if let Some(start) = data
                    .gesture_start
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_mut()
                {
                    start.resized = true;
                }
            }
            WM_MOVING => {}
            WM_EXITSIZEMOVE => {
                let start = data
                    .gesture_start
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                if let (Some(start), Some(end), Some(end_scale)) =
                    (start, rect(hwnd), scale(hwnd, data.zoom_factor()))
                    && (start.rect != end || start.scale.to_bits() != end_scale.to_bits())
                {
                    let sequence = data
                        .sequence
                        .fetch_add(1, Ordering::Relaxed)
                        .wrapping_add(1);
                    let observation = NativeGeometryGesture {
                        tool_key: data.tool_key,
                        stamp: data.stamp,
                        position: end,
                        position_scale: end_scale,
                        position_sequence: sequence,
                        size: start.resized.then_some(end),
                        size_sequence: start.resized.then_some(sequence),
                    };
                    {
                        let mut slot = data
                            .slot
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        *slot = Some(merge_gesture(slot.take(), observation));
                    }
                    (data.wake)(hwnd.0 as usize);
                }
            }
            WM_NCDESTROY => {
                if unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) }.0 as usize == reference_data {
                    let _ = unsafe { RemovePropW(hwnd, BINDING_PROPERTY_NAME) };
                }
                let _ = unsafe { RemoveWindowSubclass(hwnd, Some(utility_subclass), SUBCLASS_ID) };
                let release_raw = !data.raw_reference_released.swap(true, Ordering::AcqRel);
                let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
                if release_raw {
                    drop(unsafe { Arc::from_raw(reference_data as *const SubclassData) });
                }
                return result;
            }
            _ => {}
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    pub(super) fn attach_checked(
        target: CheckedNativeWindow,
        tool_key: crate::settings::UtilityToolKey,
        stamp: NativeGeometryStamp,
        slot: NativeGeometrySlot,
        zoom_factor: f32,
        sequence: Arc<AtomicU64>,
        wake: Arc<dyn Fn(usize) + Send + Sync>,
    ) -> Result<SubclassBinding, String> {
        let hwnd = checked_hwnd(target)?;
        if !window_bounds(hwnd).is_some_and(NativeWindowRect::is_valid) {
            return Err("the utility HWND has no measurable normal outer rectangle".into());
        }
        if !unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) }.0.is_null() {
            return Err("the utility HWND already has a geometry gesture binding".into());
        }

        let data = Arc::new(SubclassData {
            tool_key,
            stamp,
            slot,
            sequence,
            gesture_start: Mutex::new(None),
            zoom_factor_bits: AtomicU32::new(zoom_factor.to_bits()),
            raw_reference_released: AtomicBool::new(false),
            wake,
        });
        let raw_data = Arc::into_raw(Arc::clone(&data)) as usize;
        if let Err(error) =
            unsafe { SetPropW(hwnd, BINDING_PROPERTY_NAME, HANDLE(raw_data as *mut c_void)) }
        {
            drop(unsafe { Arc::from_raw(raw_data as *const SubclassData) });
            return Err(format!("SetPropW rejected the geometry binding: {error}"));
        }
        if !unsafe { SetWindowSubclass(hwnd, Some(utility_subclass), SUBCLASS_ID, raw_data) }
            .as_bool()
        {
            if unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) }.0 as usize == raw_data {
                let _ = unsafe { RemovePropW(hwnd, BINDING_PROPERTY_NAME) };
            }
            drop(unsafe { Arc::from_raw(raw_data as *const SubclassData) });
            return Err("SetWindowSubclass rejected the geometry binding".into());
        }
        Ok(SubclassBinding {
            hwnd: target.hwnd,
            raw_data,
            data,
        })
    }

    impl SubclassData {
        fn zoom_factor(&self) -> f32 {
            f32::from_bits(self.zoom_factor_bits.load(Ordering::Relaxed))
        }
    }

    pub(super) fn update_zoom(binding: &SubclassBinding, zoom_factor: f32) {
        if zoom_factor.is_finite() && zoom_factor > 0.0 {
            binding
                .data
                .zoom_factor_bits
                .store(zoom_factor.to_bits(), Ordering::Relaxed);
        }
    }

    pub(super) fn window_handle(binding: &SubclassBinding) -> Option<usize> {
        Some(binding.hwnd)
    }

    pub(super) fn binding_is_live(binding: &SubclassBinding) -> bool {
        if binding.data.raw_reference_released.load(Ordering::Acquire) {
            return false;
        }
        let hwnd = HWND(binding.hwnd as *mut c_void);
        unsafe { IsWindow(hwnd) }.as_bool()
            && unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) }.0 as usize == binding.raw_data
    }

    pub(super) fn detach(binding: SubclassBinding) {
        let hwnd = HWND(binding.hwnd as *mut c_void);
        let same_binding = unsafe { IsWindow(hwnd) }.as_bool()
            && unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) }.0 as usize == binding.raw_data;
        if same_binding {
            if unsafe { RemoveWindowSubclass(hwnd, Some(utility_subclass), SUBCLASS_ID) }.as_bool()
            {
                let same_property =
                    unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) }.0 as usize == binding.raw_data;
                if same_property {
                    let _ = unsafe { RemovePropW(hwnd, BINDING_PROPERTY_NAME) };
                }
                if !binding
                    .data
                    .raw_reference_released
                    .swap(true, Ordering::AcqRel)
                {
                    drop(unsafe { Arc::from_raw(binding.raw_data as *const SubclassData) });
                }
            } else {
                tracing::warn!(
                    hwnd = binding.hwnd,
                    "failed to detach native utility geometry subclass"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gesture(
        stamp: NativeGeometryStamp,
        pos_seq: u64,
        size_seq: Option<u64>,
    ) -> NativeGeometryGesture {
        let rect = NativeWindowRect {
            left: -400,
            top: 80,
            right: 200,
            bottom: 480,
        };
        NativeGeometryGesture {
            tool_key: UtilityToolKey::parse("hash-generator").unwrap(),
            stamp,
            position: rect,
            position_scale: 1.5,
            position_sequence: pos_seq,
            size: size_seq.map(|_| rect),
            size_sequence: size_seq,
        }
    }

    #[test]
    fn completion_slot_coalesces_position_and_explicit_size_components() {
        let stamp = NativeGeometryStamp {
            session_epoch: 4,
            mount_generation: 9,
        };
        let slot = new_native_geometry_slot();
        {
            let mut slot = slot.lock().unwrap();
            *slot = Some(merge_gesture(None, gesture(stamp, 1, Some(1))));
            *slot = Some(merge_gesture(slot.take(), gesture(stamp, 3, None)));
        }

        let sample = take_native_geometry(
            &slot,
            UtilityToolKey::parse("hash-generator").unwrap(),
            stamp,
        )
        .unwrap();
        assert_eq!(sample.position_sequence, 3);
        assert_eq!(sample.size_sequence, Some(1));
        assert!(
            take_native_geometry(
                &slot,
                UtilityToolKey::parse("hash-generator").unwrap(),
                stamp,
            )
            .is_none()
        );
    }

    #[test]
    fn completion_from_old_mount_or_session_is_discarded() {
        let slot = new_native_geometry_slot();
        let old = NativeGeometryStamp {
            session_epoch: 2,
            mount_generation: 8,
        };
        *slot.lock().unwrap() = Some(gesture(old, 1, None));
        let current = NativeGeometryStamp {
            session_epoch: 3,
            mount_generation: 9,
        };
        assert!(
            take_native_geometry(
                &slot,
                UtilityToolKey::parse("hash-generator").unwrap(),
                current,
            )
            .is_none()
        );
    }

    #[test]
    fn restore_clamps_to_nearest_negative_origin_work_area() {
        let work_area = NativeWindowRect {
            left: -1_920,
            top: -160,
            right: 0,
            bottom: 920,
        };
        let desired = NativeWindowRect {
            left: -2_400,
            top: -300,
            right: -1_400,
            bottom: 500,
        };
        let restored = clamp_outer_rect_to_work_area(desired, work_area, (420, 320)).unwrap();
        assert_eq!(restored.left, -1_920);
        assert_eq!(restored.top, -160);
        assert_eq!(restored.width(), 1_000);
        assert_eq!(restored.height(), 800);
        assert!(restored.right <= work_area.right);
        assert!(restored.bottom <= work_area.bottom);
    }
}
