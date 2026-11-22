use crate::settings::UtilityToolKey;
#[cfg(test)]
use crate::settings::{NativeWindowGeometry, PreferredWindowSize, SavedWindowPosition};
pub(crate) use crate::utility_geometry_binding::NativeWindowRect;
use crate::utility_geometry_binding::{
    NativeGeometryBinding, NativeGeometryGesture, NativeGeometrySlot, NativeGeometryStamp,
    new_native_geometry_slot,
};
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

type ObservationSlot = NativeGeometrySlot;

struct NativeRegistration {
    slot: ObservationSlot,
    binding: Option<NativeGeometryBinding>,
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
                drop(binding);
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
                .is_some_and(NativeGeometryBinding::is_live);
            if binding_is_live {
                if let Some(binding) = state
                    .registrations
                    .get(&identity)
                    .and_then(|registration| registration.binding.as_ref())
                {
                    binding.update_zoom(zoom_factor);
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

        let slot = new_native_geometry_slot();
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
        let wake = Arc::new(invalidate_egui_window);
        let binding = match NativeGeometryBinding::attach_discovered(
            title,
            root.hwnd,
            expected_bounds,
            identity.key,
            NativeGeometryStamp {
                session_epoch: identity.owner_generation,
                mount_generation: identity.host_generation,
            },
            Arc::clone(&slot),
            zoom_factor,
            Arc::clone(&self.sequence),
            wake,
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
        let window = binding.window_handle();
        if let Some(window) = window
            && state
                .claimed_windows
                .get(&window)
                .is_some_and(|claimed| *claimed != identity)
        {
            drop(binding);
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
        if !crate::utility_geometry_binding::root_is_live(root.hwnd) {
            return None;
        }
        match identity {
            Some(identity) => {
                let binding = state.registrations.get(&identity)?.binding.as_ref()?;
                binding.is_live().then(|| binding.window_handle()).flatten()
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
            .filter_map(|(identity, registration)| take_slot(&registration.slot, *identity))
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
                drop(binding);
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

fn take_slot(
    slot: &ObservationSlot,
    identity: NativeViewportIdentity,
) -> Option<NativeGeometryObservation> {
    let sample = crate::utility_geometry_binding::take_native_geometry(
        slot,
        identity.key,
        NativeGeometryStamp {
            session_epoch: identity.owner_generation,
            mount_generation: identity.host_generation,
        },
    )?;
    Some(NativeGeometryObservation {
        identity,
        position: sample.position,
        position_scale: sample.position_scale,
        position_sequence: sample.position_sequence,
        size: sample.size,
        size_sequence: sample.size_sequence,
    })
}

#[cfg(windows)]
fn invalidate_egui_window(hwnd: usize) {
    use windows::Win32::{Foundation::HWND, Graphics::Gdi::InvalidateRect};
    let _ = unsafe { InvalidateRect(HWND(hwnd as *mut _), None, false) };
}

#[cfg(not(windows))]
fn invalidate_egui_window(_: usize) {}

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

    fn slot_for(observation: NativeGeometryObservation) -> ObservationSlot {
        Arc::new(Mutex::new(Some(NativeGeometryGesture {
            tool_key: observation.identity.key,
            stamp: NativeGeometryStamp {
                session_epoch: observation.identity.owner_generation,
                mount_generation: observation.identity.host_generation,
            },
            position: observation.position,
            position_scale: observation.position_scale,
            position_sequence: observation.position_sequence,
            size: observation.size,
            size_sequence: observation.size_sequence,
        })))
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
        observer.activate_test_slot(viewport_a, slot_for(slot_a));
        observer.activate_test_slot(viewport_b, slot_for(resize_b));

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
        let old_slot = slot_for(observation(old_identity));
        observer.activate_test_slot(old_identity, old_slot);
        let current_identity = identity(2);
        observer.retire(old_identity);
        observer.activate_test_slot(current_identity, new_native_geometry_slot());
        assert!(observer.drain().is_empty());

        observer.retire(current_identity);
        assert!(observer.drain().is_empty());
    }

    #[test]
    fn retiring_one_viewport_keeps_same_tool_peer_registration_live() {
        let observer = NativeGeometryObserver::default();
        let retired_identity = identity(3);
        let live_identity = identity(4);
        observer.activate_test_slot(retired_identity, slot_for(observation(retired_identity)));
        observer.activate_test_slot(live_identity, slot_for(observation(live_identity)));

        observer.retire(retired_identity);

        assert_eq!(observer.drain(), vec![observation(live_identity)]);
    }
}
