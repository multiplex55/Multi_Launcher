use crate::settings::UtilityToolKey;
#[cfg(test)]
use crate::settings::{NativeWindowGeometry, PreferredWindowSize, SavedWindowPosition};
use eframe::egui::ViewportId;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

// The observer is event-driven and stores one fixed completion slot per live
// host. Keep its memory use bounded above the full requested surface count.
const MAX_LIVE_OBSERVERS: usize = 64;
const MAX_ATTACH_ATTEMPTS: u8 = 4;

/// Identifies one accepted native utility viewport lifetime. Diagram Studio's
/// owner and viewer generations remain independent.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct NativeViewportIdentity {
    pub(crate) key: UtilityToolKey,
    pub(crate) viewport_id: ViewportId,
    pub(crate) owner_generation: u64,
    pub(crate) host_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeWindowRect {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) right: i32,
    pub(crate) bottom: i32,
}

impl NativeWindowRect {
    pub(crate) fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub(crate) fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    pub(crate) fn is_valid(self) -> bool {
        self.width() > 0 && self.height() > 0
    }

    fn matches(self, other: Self) -> bool {
        self.left.abs_diff(other.left) <= 2
            && self.top.abs_diff(other.top) <= 2
            && self.right.abs_diff(other.right) <= 2
            && self.bottom.abs_diff(other.bottom) <= 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NativeGeometryObservation {
    pub(crate) identity: NativeViewportIdentity,
    pub(crate) position: NativeWindowRect,
    pub(crate) position_scale: f32,
    pub(crate) position_sequence: u64,
    pub(crate) size: Option<NativeWindowRect>,
    pub(crate) size_sequence: Option<u64>,
}

impl NativeGeometryObservation {
    /// Merge bounded gesture slots component-wise. Position and explicit size
    /// intent have independent event order, so a later move cannot erase a
    /// resize that has not yet been acknowledged.
    pub(crate) fn followed_by(self, next: Self) -> Self {
        if self.identity.key != next.identity.key {
            return next;
        }

        let position = if next.position_sequence > self.position_sequence {
            next
        } else {
            self
        };
        let size = if next.size_sequence > self.size_sequence {
            next
        } else {
            self
        };
        Self {
            identity: position.identity,
            position: position.position,
            position_scale: position.position_scale,
            position_sequence: position.position_sequence,
            size: size.size,
            size_sequence: size.size_sequence,
        }
    }

    /// Persist a complete geometry snapshot. A move carries forward the saved
    /// preferred physical size even when the current work area temporarily
    /// clamps the visible window. Only a completed interactive resize replaces
    /// that size.
    #[cfg(test)]
    fn geometry(self, previous: Option<NativeWindowGeometry>) -> Option<NativeWindowGeometry> {
        if !self.position.is_valid() || !valid_scale(self.position_scale) {
            return None;
        }

        let size = if let Some(size) = self.size {
            if !size.is_valid() {
                return None;
            }
            PreferredWindowSize::new(
                size.width() as f32 / self.position_scale,
                size.height() as f32 / self.position_scale,
            )
            .ok()?
        } else if let Some(previous) = previous {
            PreferredWindowSize::new(
                previous.size.width * previous.scale_factor / self.position_scale,
                previous.size.height * previous.scale_factor / self.position_scale,
            )
            .ok()?
        } else {
            PreferredWindowSize::new(
                self.position.width() as f32 / self.position_scale,
                self.position.height() as f32 / self.position_scale,
            )
            .ok()?
        };
        let position = SavedWindowPosition::new(
            self.position.left as f32 / self.position_scale,
            self.position.top as f32 / self.position_scale,
        )
        .ok()?;
        NativeWindowGeometry::new(position, size, self.position_scale).ok()
    }
}

fn gesture_changed(
    start: NativeWindowRect,
    end: NativeWindowRect,
    start_scale: f32,
    end_scale: f32,
) -> bool {
    start != end || start_scale.to_bits() != end_scale.to_bits()
}

fn valid_scale(scale: f32) -> bool {
    scale.is_finite() && scale > 0.0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RootWindowIdentity {
    pub(crate) hwnd: usize,
    pub(crate) generation: u64,
}

type ObservationSlot = Arc<Mutex<Option<NativeGeometryObservation>>>;

struct NativeRegistration {
    slot: ObservationSlot,
    binding: Option<native::SubclassBinding>,
    window: Option<usize>,
}

#[derive(Default)]
struct ObserverState {
    root: Option<RootWindowIdentity>,
    active: HashSet<NativeViewportIdentity>,
    registrations: HashMap<NativeViewportIdentity, NativeRegistration>,
    claimed_windows: HashMap<usize, NativeViewportIdentity>,
    reported_failures: HashSet<NativeViewportIdentity>,
    attempts: HashMap<NativeViewportIdentity, (NativeWindowRect, u8)>,
}

impl Drop for ObserverState {
    fn drop(&mut self) {
        for (_, mut registration) in self.registrations.drain() {
            if let Some(binding) = registration.binding.take() {
                native::detach(binding);
            }
        }
    }
}

/// A bounded, event-driven observer. Each live viewport owns one replaceable
/// completion slot; no geometry polling or permanent window hook is used.
#[derive(Clone, Default)]
pub(crate) struct NativeGeometryObserver {
    state: Arc<Mutex<ObserverState>>,
    sequence: Arc<AtomicU64>,
}

impl NativeGeometryObserver {
    pub(crate) fn root(&self) -> Option<RootWindowIdentity> {
        self.lock().root
    }

    pub(crate) fn next_sequence(&self) -> u64 {
        self.sequence
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }

    pub(crate) fn set_root(&self, root: Option<RootWindowIdentity>) {
        let mut state = self.lock();
        if state.root == root {
            return;
        }
        Self::retire_all_locked(&mut state);
        state.active.clear();
        state.reported_failures.clear();
        state.attempts.clear();
        state.root = root;
    }

    pub(crate) fn ensure_attached(
        &self,
        title: &str,
        identity: NativeViewportIdentity,
        root: Option<RootWindowIdentity>,
        zoom_factor: f32,
        expected_bounds: NativeWindowRect,
    ) {
        let mut state = self.lock();
        if state.root != root {
            return;
        }
        if state.active.contains(&identity) {
            let binding_is_live = state
                .registrations
                .get(&identity)
                .and_then(|registration| registration.binding.as_ref())
                .is_some_and(native::binding_is_live);
            if binding_is_live {
                if let Some(binding) = state
                    .registrations
                    .get(&identity)
                    .and_then(|registration| registration.binding.as_ref())
                {
                    native::update_zoom(binding, zoom_factor);
                }
                return;
            }
            Self::retire_identity_locked(&mut state, identity);
        }

        if state.registrations.len() >= MAX_LIVE_OBSERVERS {
            if state.reported_failures.insert(identity) {
                tracing::warn!(
                    tool = identity.key.as_str(),
                    limit = MAX_LIVE_OBSERVERS,
                    "native utility geometry observer limit reached"
                );
            }
            return;
        }

        let slot = Arc::new(Mutex::new(None));
        let Some(root) = root else {
            return;
        };
        if !state.attempts.contains_key(&identity) && state.attempts.len() >= MAX_LIVE_OBSERVERS {
            return;
        }
        let (last_bounds, attempts) = state
            .attempts
            .entry(identity)
            .or_insert((expected_bounds, 0));
        if !last_bounds.matches(expected_bounds) {
            *last_bounds = expected_bounds;
            *attempts = 0;
        }
        if *attempts >= MAX_ATTACH_ATTEMPTS {
            return;
        }
        *attempts += 1;
        let binding = match native::attach(
            title,
            root,
            expected_bounds,
            identity,
            Arc::clone(&slot),
            zoom_factor,
            Arc::clone(&self.sequence),
        ) {
            Ok(Some(binding)) => binding,
            Ok(None) => return,
            Err(error) => {
                if state.reported_failures.insert(identity) {
                    tracing::warn!(tool = identity.key.as_str(), %error, "could not attach native utility geometry observer");
                }
                return;
            }
        };
        let window = native::window_handle(&binding);
        if let Some(window) = window
            && state
                .claimed_windows
                .get(&window)
                .is_some_and(|claimed| *claimed != identity)
        {
            native::detach(binding);
            if state.reported_failures.insert(identity) {
                tracing::warn!(
                    tool = identity.key.as_str(),
                    "native utility viewport HWND is already claimed by another live host"
                );
            }
            return;
        }
        state.reported_failures.remove(&identity);
        state.attempts.remove(&identity);
        state.active.insert(identity);
        if let Some(window) = window {
            state.claimed_windows.insert(window, identity);
        }
        state.registrations.insert(
            identity,
            NativeRegistration {
                slot,
                binding: Some(binding),
                window,
            },
        );
    }

    #[cfg(windows)]
    pub(crate) fn picker_hwnd(&self, identity: Option<NativeViewportIdentity>) -> Option<usize> {
        let state = self.lock();
        let root = state.root?;
        if !native::root_is_live(root) {
            return None;
        }
        match identity {
            Some(identity) => {
                let binding = state.registrations.get(&identity)?.binding.as_ref()?;
                native::binding_is_live(binding)
                    .then(|| native::window_handle(binding))
                    .flatten()
            }
            None => Some(root.hwnd),
        }
    }

    pub(crate) fn drain(&self) -> Vec<NativeGeometryObservation> {
        let state = self.lock();
        let observations = state
            .registrations
            .iter()
            .filter(|(identity, _)| state.active.contains(identity))
            .filter_map(|(_, registration)| take_slot(&registration.slot))
            .collect::<Vec<_>>();
        let mut by_key = HashMap::<UtilityToolKey, NativeGeometryObservation>::new();
        for observation in observations {
            by_key
                .entry(observation.identity.key)
                .and_modify(|current| *current = current.followed_by(observation))
                .or_insert(observation);
        }
        by_key.into_values().collect()
    }

    pub(crate) fn retire(&self, identity: NativeViewportIdentity) {
        Self::retire_identity_locked(&mut self.lock(), identity);
    }

    pub(crate) fn retire_all(&self) {
        let mut state = self.lock();
        Self::retire_all_locked(&mut state);
        state.active.clear();
        state.attempts.clear();
    }

    fn retire_identity_locked(state: &mut ObserverState, identity: NativeViewportIdentity) {
        state.active.remove(&identity);
        state.attempts.remove(&identity);
        state.reported_failures.remove(&identity);
        if let Some(mut registration) = state.registrations.remove(&identity) {
            if let Some(window) = registration.window
                && state.claimed_windows.get(&window) == Some(&identity)
            {
                state.claimed_windows.remove(&window);
            }
            if let Some(binding) = registration.binding.take() {
                native::detach(binding);
            }
        }
    }

    fn retire_all_locked(state: &mut ObserverState) {
        let identities = state.registrations.keys().copied().collect::<Vec<_>>();
        for identity in identities {
            Self::retire_identity_locked(state, identity);
        }
    }

    fn lock(&self) -> MutexGuard<'_, ObserverState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    fn activate_test_slot(&self, identity: NativeViewportIdentity, slot: ObservationSlot) {
        let mut state = self.lock();
        state.active.insert(identity);
        state.registrations.insert(
            identity,
            NativeRegistration {
                slot,
                binding: None,
                window: None,
            },
        );
    }
}

fn take_slot(slot: &ObservationSlot) -> Option<NativeGeometryObservation> {
    slot.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

#[cfg(windows)]
mod native {
    use super::{
        NativeGeometryObservation, NativeViewportIdentity, NativeWindowRect, ObservationSlot,
        RootWindowIdentity, gesture_changed,
    };
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::InvalidateRect;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GA_ROOT, GA_ROOTOWNER, GetAncestor, GetPropW, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
        IsZoomed, RemovePropW, SetPropW, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVING,
        WM_NCDESTROY, WM_SIZING,
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
        identity: NativeViewportIdentity,
        slot: ObservationSlot,
        sequence: Arc<AtomicU64>,
        gesture_start: Mutex<Option<GestureStart>>,
        zoom_factor_bits: AtomicU32,
        raw_reference_released: AtomicBool,
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
        matches: Vec<HWND>,
    }

    unsafe extern "system" fn find_exact_child(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let search = unsafe { &mut *(parameter.0 as *mut Search<'_>) };
        if hwnd == search.root || !unsafe { IsWindow(hwnd) }.as_bool() {
            return BOOL(1);
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        // Pinned eframe creates secondary winit windows without assigning a
        // Win32 owner. Accept that top-level case only with the exact title,
        // current UI thread, process, and unique HWND match established here.
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
        if copied > 0 && &title[..copied as usize] == search.title {
            search.matches.push(hwnd);
        }
        BOOL(1)
    }

    pub(super) fn root_is_live(identity: RootWindowIdentity) -> bool {
        let root = HWND(identity.hwnd as *mut c_void);
        let mut process_id = 0;
        identity.hwnd != 0
            && unsafe { IsWindow(root) }.as_bool()
            && unsafe { GetAncestor(root, GA_ROOT) } == root
            && unsafe { GetWindowThreadProcessId(root, Some(&mut process_id)) } != 0
            && process_id == std::process::id()
    }

    fn find_child(title: &str, root_identity: RootWindowIdentity) -> Result<Option<HWND>, String> {
        let root = HWND(root_identity.hwnd as *mut c_void);
        if root_identity.hwnd == 0
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
            [hwnd] => Ok(Some(*hwnd)),
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
            WM_MOVING => {
                // A move that triggers work-area clamping can change the
                // observed outer dimensions without being a user resize.
            }
            WM_EXITSIZEMOVE => {
                let start = data
                    .gesture_start
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                if let (Some(start), Some(end), Some(end_scale)) =
                    (start, rect(hwnd), scale(hwnd, data.zoom_factor()))
                    && gesture_changed(start.rect, end, start.scale, end_scale)
                {
                    let sequence = data
                        .sequence
                        .fetch_add(1, Ordering::Relaxed)
                        .wrapping_add(1);
                    let observation = NativeGeometryObservation {
                        identity: data.identity,
                        position: end,
                        position_scale: end_scale,
                        position_sequence: sequence,
                        size: start.resized.then_some(end),
                        size_sequence: start.resized.then_some(sequence),
                    };
                    let mut slot = data
                        .slot
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    *slot = Some(
                        slot.take()
                            .map_or(observation, |current| current.followed_by(observation)),
                    );
                    // The completion itself has no egui redraw event in the
                    // pinned winit backend. Invalidate only this owning window
                    // to schedule the frame that drains the bounded slot.
                    let _ = unsafe { InvalidateRect(hwnd, None, false) };
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

    pub(super) fn attach(
        title: &str,
        root: RootWindowIdentity,
        expected_bounds: NativeWindowRect,
        identity: NativeViewportIdentity,
        slot: ObservationSlot,
        zoom_factor: f32,
        sequence: Arc<AtomicU64>,
    ) -> Result<Option<SubclassBinding>, String> {
        let Some(hwnd) = find_child(title, root)? else {
            return Ok(None);
        };
        let Some(bounds) = window_bounds(hwnd) else {
            return Ok(None);
        };
        // Match the retained viewport geometry as well as the unique title,
        // root process, and UI thread before binding a native child HWND.
        if !bounds.matches(expected_bounds) {
            return Ok(None);
        }
        let mut root_process = 0;
        let root_thread = unsafe {
            GetWindowThreadProcessId(HWND(root.hwnd as *mut c_void), Some(&mut root_process))
        };
        let mut child_process = 0;
        let child_thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut child_process)) };
        if root_process != std::process::id()
            || child_process != root_process
            || child_thread != root_thread
            || {
                let owner = unsafe { GetAncestor(hwnd, GA_ROOTOWNER) };
                owner != HWND(root.hwnd as *mut c_void) && owner != hwnd
            }
        {
            return Err("the native viewport process or root-owner identity changed".into());
        }

        if !unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) }.0.is_null() {
            return Err("the native utility viewport already has an observer binding".into());
        }

        let data = Arc::new(SubclassData {
            identity,
            slot,
            sequence,
            gesture_start: Mutex::new(None),
            zoom_factor_bits: AtomicU32::new(zoom_factor.to_bits()),
            raw_reference_released: AtomicBool::new(false),
        });
        let raw_data = Arc::into_raw(Arc::clone(&data)) as usize;
        if let Err(error) =
            unsafe { SetPropW(hwnd, BINDING_PROPERTY_NAME, HANDLE(raw_data as *mut c_void)) }
        {
            drop(unsafe { Arc::from_raw(raw_data as *const SubclassData) });
            return Err(format!(
                "SetPropW rejected the utility viewport observer identity: {error}"
            ));
        }
        if !unsafe { SetWindowSubclass(hwnd, Some(utility_subclass), SUBCLASS_ID, raw_data) }
            .as_bool()
        {
            if unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) }.0 as usize == raw_data {
                let _ = unsafe { RemovePropW(hwnd, BINDING_PROPERTY_NAME) };
            }
            drop(unsafe { Arc::from_raw(raw_data as *const SubclassData) });
            return Err("SetWindowSubclass rejected the utility viewport observer".into());
        }
        Ok(Some(SubclassBinding {
            hwnd: hwnd.0 as usize,
            raw_data,
            data,
        }))
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

#[cfg(not(windows))]
mod native {
    use super::{NativeViewportIdentity, NativeWindowRect, ObservationSlot, RootWindowIdentity};
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    pub(super) struct SubclassBinding;

    pub(super) fn attach(
        _title: &str,
        _root: RootWindowIdentity,
        _expected_bounds: NativeWindowRect,
        _identity: NativeViewportIdentity,
        _slot: ObservationSlot,
        _zoom_factor: f32,
        _sequence: Arc<AtomicU64>,
    ) -> Result<Option<SubclassBinding>, String> {
        Ok(None)
    }

    pub(super) fn update_zoom(_binding: &SubclassBinding, _zoom_factor: f32) {}

    pub(super) fn window_handle(_binding: &SubclassBinding) -> Option<usize> {
        None
    }

    pub(super) fn binding_is_live(_binding: &SubclassBinding) -> bool {
        true
    }

    pub(super) fn detach(_binding: SubclassBinding) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::UtilityToolKey;
    use eframe::egui;

    fn identity(generation: u64) -> NativeViewportIdentity {
        NativeViewportIdentity {
            key: UtilityToolKey::parse("calculation-pad").unwrap(),
            viewport_id: egui::ViewportId::from_hash_of(("native-geometry-test", generation)),
            owner_generation: 0,
            host_generation: generation,
        }
    }

    fn observation(identity: NativeViewportIdentity) -> NativeGeometryObservation {
        NativeGeometryObservation {
            identity,
            position: NativeWindowRect {
                left: 10,
                top: 10,
                right: 510,
                bottom: 410,
            },
            position_scale: 1.0,
            position_sequence: 1,
            size: None,
            size_sequence: None,
        }
    }

    fn moved(
        identity: NativeViewportIdentity,
        position: NativeWindowRect,
        scale: f32,
        sequence: u64,
    ) -> NativeGeometryObservation {
        NativeGeometryObservation {
            identity,
            position,
            position_scale: scale,
            position_sequence: sequence,
            size: None,
            size_sequence: None,
        }
    }

    fn resized(
        identity: NativeViewportIdentity,
        rect: NativeWindowRect,
        scale: f32,
        sequence: u64,
    ) -> NativeGeometryObservation {
        NativeGeometryObservation {
            identity,
            position: rect,
            position_scale: scale,
            position_sequence: sequence,
            size: Some(rect),
            size_sequence: Some(sequence),
        }
    }

    fn geometry(size: PreferredWindowSize, scale: f32) -> NativeWindowGeometry {
        NativeWindowGeometry::new(
            SavedWindowPosition::new(-500.0, 200.0).unwrap(),
            size,
            scale,
        )
        .unwrap()
    }

    #[test]
    fn move_preserves_preferred_size_when_visible_window_was_clamped() {
        let old = geometry(PreferredWindowSize::new(1_200.0, 800.0).unwrap(), 1.5);
        let observation = moved(
            identity(1),
            NativeWindowRect {
                left: -700,
                top: 180,
                right: -250,
                bottom: 480,
            },
            1.5,
            1,
        );
        let saved = observation.geometry(Some(old)).unwrap();
        assert_eq!(saved.position.x, -700.0 / 1.5);
        assert_eq!(saved.size, old.size);
        assert_eq!(saved.scale_factor, 1.5);
    }

    #[test]
    fn move_across_dpi_keeps_preferred_physical_size_and_updates_position_scale() {
        let old = geometry(PreferredWindowSize::new(1_200.0, 800.0).unwrap(), 1.5);
        let observation = moved(
            identity(1),
            NativeWindowRect {
                left: 1_920,
                top: 100,
                right: 3_120,
                bottom: 900,
            },
            2.0,
            1,
        );
        let saved = observation.geometry(Some(old)).unwrap();
        assert_eq!(saved.position.x, 960.0);
        assert_eq!(saved.size.width, 900.0);
        assert_eq!(saved.size.height, 600.0);
        assert_eq!(saved.scale_factor, 2.0);
    }

    #[test]
    fn resize_updates_size_but_invalid_or_minimized_geometry_is_rejected() {
        let observation = resized(
            identity(1),
            NativeWindowRect {
                left: 20,
                top: 40,
                right: 1_020,
                bottom: 740,
            },
            1.0,
            1,
        );
        let saved = observation.geometry(None).unwrap();
        assert_eq!(
            saved.size,
            PreferredWindowSize::new(1_000.0, 700.0).unwrap()
        );
        assert!(
            NativeGeometryObservation {
                position: NativeWindowRect {
                    right: 20,
                    bottom: 40,
                    ..observation.position
                },
                size: Some(NativeWindowRect {
                    right: 20,
                    bottom: 40,
                    ..observation.position
                }),
                ..observation
            }
            .geometry(None)
            .is_none()
        );
        assert!(
            NativeGeometryObservation {
                position_scale: f32::NAN,
                ..observation
            }
            .geometry(None)
            .is_none()
        );
    }

    #[test]
    fn coalesced_resize_then_clamped_move_keeps_explicit_size_and_latest_position() {
        let identity = identity(8);
        let resize = resized(
            identity,
            NativeWindowRect {
                left: -900,
                top: 100,
                right: -100,
                bottom: 700,
            },
            1.0,
            1,
        );
        let clamped_move = moved(
            identity,
            NativeWindowRect {
                left: -500,
                top: 180,
                right: 0,
                bottom: 480,
            },
            1.0,
            3,
        );
        let merged = resize.followed_by(clamped_move);
        let saved = merged.geometry(None).unwrap();

        assert_eq!(merged.position_sequence, 3);
        assert_eq!(merged.size_sequence, Some(1));
        assert_eq!(saved.position.x, -500.0);
        assert_eq!(saved.size, PreferredWindowSize::new(800.0, 600.0).unwrap());
    }

    #[test]
    fn same_tool_viewports_keep_latest_position_and_latest_explicit_size() {
        let viewport_a = identity(10);
        let viewport_b = NativeViewportIdentity {
            viewport_id: egui::ViewportId::from_hash_of("native-geometry-peer-b"),
            host_generation: 11,
            ..viewport_a
        };
        let resize_a = resized(
            viewport_a,
            NativeWindowRect {
                left: 0,
                top: 0,
                right: 900,
                bottom: 700,
            },
            1.0,
            1,
        );
        let resize_b = resized(
            viewport_b,
            NativeWindowRect {
                left: -1200,
                top: 30,
                right: -200,
                bottom: 780,
            },
            1.0,
            2,
        );
        let move_a = moved(
            viewport_a,
            NativeWindowRect {
                left: 120,
                top: 80,
                right: 620,
                bottom: 380,
            },
            1.0,
            3,
        );
        let slot_a = resize_a.followed_by(move_a);
        let observer = NativeGeometryObserver::default();
        observer.activate_test_slot(viewport_a, Arc::new(Mutex::new(Some(slot_a))));
        observer.activate_test_slot(viewport_b, Arc::new(Mutex::new(Some(resize_b))));

        let drained = observer.drain();
        assert_eq!(drained.len(), 1, "same-key viewport slots are grouped");
        let merged = drained[0];
        let saved = merged.geometry(None).unwrap();
        assert_eq!(merged.position_sequence, 3);
        assert_eq!(merged.size_sequence, Some(2));
        assert_eq!(saved.position.x, 120.0);
        assert_eq!(saved.size, PreferredWindowSize::new(1000.0, 750.0).unwrap());
    }

    #[test]
    fn cancelled_gesture_with_unchanged_pose_is_not_an_observation() {
        let rect = NativeWindowRect {
            left: -300,
            top: 20,
            right: 500,
            bottom: 620,
        };
        assert!(!gesture_changed(rect, rect, 1.5, 1.5));
    }

    #[test]
    fn retired_and_stale_generation_observations_are_discarded() {
        let observer = NativeGeometryObserver::default();
        let old_identity = identity(1);
        let old_slot = Arc::new(Mutex::new(Some(observation(old_identity))));
        observer.activate_test_slot(old_identity, old_slot);
        let current_identity = identity(2);
        observer.retire(old_identity);
        observer.activate_test_slot(current_identity, Arc::new(Mutex::new(None)));
        assert!(observer.drain().is_empty());

        observer.retire(current_identity);
        assert!(observer.drain().is_empty());
    }

    #[test]
    fn retiring_one_viewport_keeps_same_tool_peer_registration_live() {
        let observer = NativeGeometryObserver::default();
        let retired_identity = identity(3);
        let live_identity = identity(4);
        observer.activate_test_slot(
            retired_identity,
            Arc::new(Mutex::new(Some(observation(retired_identity)))),
        );
        observer.activate_test_slot(
            live_identity,
            Arc::new(Mutex::new(Some(observation(live_identity)))),
        );

        observer.retire(retired_identity);

        assert_eq!(observer.drain(), vec![observation(live_identity)]);
    }
}
