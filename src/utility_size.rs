//! Shared persistence and acknowledgement state for utility-window geometry.
//!
//! Frontend adapters own viewport discovery and presentation. This store owns
//! accepted size/geometry writes, their revisions, and publication of the
//! canonical settings writer's current acknowledgements.

use crate::SettingsSaveWorkerRegistry;
use crate::settings::utility_dialog_sizes::UtilitySizeMutation;
use crate::settings::{
    NativeWindowGeometry, PreferredWindowSize, UtilityDialogSizes, UtilitySizeAcknowledgement,
    UtilitySizePreference, UtilitySizeRevision, UtilityToolKey,
};
use std::collections::{HashMap, HashSet};

/// A physical-pixel rectangle reported by a native-window observer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalWindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PhysicalWindowRect {
    fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    fn is_valid(self) -> bool {
        self.width() > 0 && self.height() > 0
    }
}

/// A native observation carries independent position and completed-resize
/// sequence numbers. A later move must not erase an unacknowledged resize.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UtilityNativeGeometrySample {
    pub position: PhysicalWindowRect,
    pub position_scale: f32,
    pub position_sequence: u64,
    pub size: Option<PhysicalWindowRect>,
    pub size_sequence: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UtilitySizeWriteFailure {
    pub key: UtilityToolKey,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UtilitySizeStatus {
    pub preference: UtilitySizePreference,
    pub acknowledged_revision: Option<UtilitySizeRevision>,
    pub pending_revision: Option<UtilitySizeRevision>,
    pub failure: Option<(UtilitySizeRevision, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UtilitySizeAcceptedAcknowledgement {
    pub acknowledgement: UtilitySizeAcknowledgement,
    /// A real user resize was accepted at this revision or an older one.
    pub locally_resized: bool,
    /// A reset was accepted at this revision or an older one.
    pub explicitly_reset: Option<UtilitySizeRevision>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UtilitySizePoll {
    pub accepted: Vec<UtilitySizeAcceptedAcknowledgement>,
    /// The adapter should request its normal acknowledgement poll repaint.
    pub awaiting_acknowledgement: bool,
    /// An acknowledgement failed and the adapter should repaint immediately.
    pub write_failed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum NativeGeometryIntent {
    Geometry {
        geometry: NativeWindowGeometry,
        position_sequence: u64,
        size_sequence: u64,
    },
    Reset {
        sequence: u64,
    },
}

impl NativeGeometryIntent {
    fn from_committed(geometry: Option<NativeWindowGeometry>) -> Self {
        geometry.map_or(Self::Reset { sequence: 0 }, |geometry| Self::Geometry {
            geometry,
            position_sequence: 0,
            size_sequence: 0,
        })
    }

    fn geometry(self) -> Option<NativeWindowGeometry> {
        match self {
            Self::Geometry { geometry, .. } => Some(geometry),
            Self::Reset { .. } => None,
        }
    }

    fn followed_by(
        self,
        observation: UtilityNativeGeometrySample,
        factory_size: Option<PreferredWindowSize>,
    ) -> Option<Self> {
        if !observation.position.is_valid()
            || !observation.position_scale.is_finite()
            || observation.position_scale <= 0.0
        {
            return None;
        }
        let (previous, previous_position_sequence, previous_size_sequence, reset_sequence) =
            match self {
                Self::Geometry {
                    geometry,
                    position_sequence,
                    size_sequence,
                } => (Some(geometry), position_sequence, size_sequence, 0),
                Self::Reset { sequence } => (None, sequence, sequence, sequence),
            };
        let position_is_new = observation.position_sequence > previous_position_sequence
            && observation.position_sequence > reset_sequence;
        let size_event = observation
            .size_sequence
            .filter(|sequence| *sequence > previous_size_sequence && *sequence > reset_sequence);
        if !position_is_new && size_event.is_none() {
            return None;
        }

        let scale = if position_is_new {
            observation.position_scale
        } else {
            previous?.scale_factor
        };
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let position = if position_is_new {
            crate::settings::SavedWindowPosition::new(
                observation.position.left as f32 / scale,
                observation.position.top as f32 / scale,
            )
            .ok()?
        } else {
            previous?.position
        };
        let size = if size_event.is_some() {
            let rect = observation.size?;
            if !rect.is_valid() {
                return None;
            }
            PreferredWindowSize::new(rect.width() as f32 / scale, rect.height() as f32 / scale)
                .ok()?
        } else if let Some(previous) = previous {
            if position_is_new {
                PreferredWindowSize::new(
                    previous.size.width * previous.scale_factor / scale,
                    previous.size.height * previous.scale_factor / scale,
                )
                .ok()?
            } else {
                previous.size
            }
        } else {
            factory_size?
        };
        let geometry = NativeWindowGeometry::new(position, size, scale).ok()?;
        Some(Self::Geometry {
            geometry,
            position_sequence: if position_is_new {
                observation.position_sequence
            } else {
                previous_position_sequence
            },
            size_sequence: size_event.unwrap_or(previous_size_sequence),
        })
    }
}

/// Frontend-neutral owner for canonical utility-size preferences and write
/// acknowledgements. Adapters can keep it inside their existing short state
/// mutex and own viewport/window state separately.
pub struct UtilitySizeStore {
    settings_path: String,
    workers: SettingsSaveWorkerRegistry,
    committed: UtilityDialogSizes,
    accepted_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    waiting_for_ack: HashMap<UtilityToolKey, UtilitySizeRevision>,
    local_resize_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    reset_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    accepted_reset_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    native_geometry_intents: HashMap<UtilityToolKey, NativeGeometryIntent>,
    known_keys: HashSet<UtilityToolKey>,
    reported_failures: HashMap<UtilityToolKey, UtilitySizeRevision>,
}

impl UtilitySizeStore {
    pub fn new(
        settings_path: String,
        workers: SettingsSaveWorkerRegistry,
        committed: UtilityDialogSizes,
    ) -> Self {
        Self {
            settings_path,
            workers,
            committed,
            accepted_revisions: HashMap::new(),
            waiting_for_ack: HashMap::new(),
            local_resize_revisions: HashMap::new(),
            reset_revisions: HashMap::new(),
            accepted_reset_revisions: HashMap::new(),
            native_geometry_intents: HashMap::new(),
            known_keys: HashSet::new(),
            reported_failures: HashMap::new(),
        }
    }

    /// Rebind the writer identity while preserving the existing in-session
    /// cache and revision state, matching the egui adapter's current behavior.
    pub fn rebind(&mut self, settings_path: String, workers: SettingsSaveWorkerRegistry) {
        self.settings_path = settings_path;
        self.workers = workers;
    }

    pub fn preference(&self, key: UtilityToolKey) -> UtilitySizePreference {
        self.committed.preference(key)
    }

    pub fn native_geometry(&self, key: UtilityToolKey) -> Option<NativeWindowGeometry> {
        self.committed.native_geometry(key)
    }

    pub fn committed_sizes(&self) -> &UtilityDialogSizes {
        &self.committed
    }

    pub fn acknowledged_revision(&self, key: UtilityToolKey) -> Option<UtilitySizeRevision> {
        self.accepted_revisions.get(&key).copied()
    }

    pub fn accepted_reset_revision(&self, key: UtilityToolKey) -> Option<UtilitySizeRevision> {
        self.accepted_reset_revisions.get(&key).copied()
    }

    pub fn has_local_resize(&self, key: UtilityToolKey) -> bool {
        self.local_resize_revisions.contains_key(&key)
    }

    pub fn track_key(&mut self, key: UtilityToolKey) {
        self.known_keys.insert(key);
    }

    /// Submit a completed user resize and return the exact accepted revision.
    pub fn set_preferred_size(
        &mut self,
        key: UtilityToolKey,
        size: PreferredWindowSize,
    ) -> Result<UtilitySizeRevision, String> {
        let revision = self.queue_mutation(key, UtilitySizeMutation::Set(size))?;
        self.local_resize_revisions.insert(key, revision);
        Ok(revision)
    }

    /// Submit native outer geometry independently from the docked size.
    fn set_native_geometry(
        &mut self,
        key: UtilityToolKey,
        geometry: NativeWindowGeometry,
    ) -> Result<UtilitySizeRevision, String> {
        self.queue_mutation(key, UtilitySizeMutation::SetNative(geometry))
    }

    /// Merge one physical native observation with the latest accepted or
    /// committed intent. Returns `None` when it adds no durable component.
    pub fn observe_native_geometry(
        &mut self,
        key: UtilityToolKey,
        observation: UtilityNativeGeometrySample,
        factory_size: Option<PreferredWindowSize>,
    ) -> Result<Option<UtilitySizeRevision>, String> {
        let current = self
            .native_geometry_intents
            .get(&key)
            .copied()
            .unwrap_or_else(|| {
                NativeGeometryIntent::from_committed(self.committed.native_geometry(key))
            });
        let Some(next) = current.followed_by(observation, factory_size) else {
            return Ok(None);
        };
        let Some(geometry) = next.geometry() else {
            return Ok(None);
        };
        let revision = self.set_native_geometry(key, geometry)?;
        self.native_geometry_intents.insert(key, next);
        Ok(Some(revision))
    }

    /// Submit an explicit reset and preserve its sequence against later native
    /// observations until a current writer acknowledgement accepts it.
    pub fn reset(
        &mut self,
        key: UtilityToolKey,
        native_sequence: u64,
    ) -> Result<UtilitySizeRevision, String> {
        self.known_keys.insert(key);
        let revision = self.queue_mutation(key, UtilitySizeMutation::Reset)?;
        self.reset_revisions.insert(key, revision);
        self.native_geometry_intents.insert(
            key,
            NativeGeometryIntent::Reset {
                sequence: native_sequence,
            },
        );
        self.local_resize_revisions.remove(&key);
        Ok(revision)
    }

    /// Consume successful current acknowledgements and advance pending state.
    /// The adapter uses accepted provenance to decide whether an open viewport
    /// needs a restore without giving this store any GUI types.
    pub fn poll(&mut self) -> Result<UtilitySizePoll, String> {
        let mut poll = UtilitySizePoll::default();
        for acknowledgement in self.workers.drain_utility_size_acknowledgements() {
            let key = acknowledgement.key;
            let revision = acknowledgement.revision;
            if self.workers.utility_size_latest_revision(key) != Some(revision) {
                continue;
            }

            self.committed
                .replace_committed(
                    key,
                    acknowledgement.preference,
                    acknowledgement.native_geometry,
                )
                .map_err(|error| error.to_string())?;
            self.accepted_revisions.insert(key, revision);
            self.waiting_for_ack.remove(&key);

            let locally_resized = self
                .local_resize_revisions
                .get(&key)
                .is_some_and(|resize_revision| *resize_revision <= revision);
            if locally_resized {
                self.local_resize_revisions.remove(&key);
            }
            let explicitly_reset = self
                .reset_revisions
                .get(&key)
                .filter(|reset_revision| **reset_revision <= revision)
                .copied();
            if let Some(reset_revision) = explicitly_reset {
                self.accepted_reset_revisions.insert(key, reset_revision);
                self.reset_revisions.remove(&key);
            }

            poll.accepted.push(UtilitySizeAcceptedAcknowledgement {
                acknowledgement,
                locally_resized,
                explicitly_reset,
            });
        }

        let waiting = std::mem::take(&mut self.waiting_for_ack);
        for (key, revision) in waiting {
            let revision = self
                .workers
                .utility_size_latest_revision(key)
                .unwrap_or(revision);
            if self.accepted_revisions.get(&key) == Some(&revision) {
                continue;
            }
            if self.workers.utility_size_write_failure(key).is_some() {
                poll.write_failed = true;
                continue;
            }
            self.waiting_for_ack.insert(key, revision);
            poll.awaiting_acknowledgement = true;
        }
        Ok(poll)
    }

    pub fn status(&self, key: UtilityToolKey) -> UtilitySizeStatus {
        let latest_revision = self.workers.utility_size_latest_revision(key);
        let failure = self.workers.utility_size_write_failure(key);
        let pending_revision = if failure.is_none() {
            self.waiting_for_ack.get(&key).copied().filter(|revision| {
                Some(*revision) == latest_revision
                    && self.accepted_revisions.get(&key) != Some(revision)
            })
        } else {
            None
        };
        UtilitySizeStatus {
            preference: self.committed.preference(key),
            acknowledged_revision: self.accepted_revisions.get(&key).copied(),
            pending_revision,
            failure,
        }
    }

    /// Report each current writer failure once; a later revision can report a
    /// new failure, while an explicit retry or accepted write clears the old.
    pub fn take_new_write_failures(&mut self) -> Vec<UtilitySizeWriteFailure> {
        let mut failures = Vec::new();
        for key in self.known_keys.iter().copied() {
            match self.workers.utility_size_write_failure(key) {
                Some((revision, message)) => {
                    if self.reported_failures.get(&key) != Some(&revision) {
                        self.reported_failures.insert(key, revision);
                        failures.push(UtilitySizeWriteFailure { key, message });
                    }
                }
                None => {
                    self.reported_failures.remove(&key);
                }
            }
        }
        failures.sort_by_key(|failure| failure.key);
        failures
    }

    /// Retry retained writer failures only from an explicit controlled event.
    pub fn retry_failed(&mut self) -> Result<(), String> {
        let failed = self
            .known_keys
            .iter()
            .filter_map(|key| {
                self.workers
                    .utility_size_write_failure(*key)
                    .map(|(revision, _)| (*key, revision))
            })
            .collect::<Vec<_>>();
        self.workers
            .retry_utility_size_mutations(self.settings_path.clone())?;
        for (key, failed_revision) in failed {
            let latest_revision = self.workers.utility_size_latest_revision(key);
            if latest_revision == Some(failed_revision)
                && self.accepted_revisions.get(&key) != Some(&failed_revision)
            {
                self.waiting_for_ack.insert(key, failed_revision);
            }
        }
        Ok(())
    }

    pub fn has_pending_acknowledgement(&self) -> bool {
        !self.waiting_for_ack.is_empty()
    }

    fn queue_mutation(
        &mut self,
        key: UtilityToolKey,
        mutation: UtilitySizeMutation,
    ) -> Result<UtilitySizeRevision, String> {
        let revision =
            self.workers
                .queue_utility_size_mutation(self.settings_path.clone(), key, mutation)?;
        self.known_keys.insert(key);
        self.waiting_for_ack.insert(key, revision);
        Ok(revision)
    }

    #[cfg(test)]
    pub(crate) fn test_set_preferred_size(
        &mut self,
        key: UtilityToolKey,
        size: PreferredWindowSize,
    ) {
        self.committed
            .set(key, size)
            .expect("test utility window size is valid");
    }

    #[cfg(test)]
    pub(crate) fn test_set_native_geometry(
        &mut self,
        key: UtilityToolKey,
        geometry: NativeWindowGeometry,
    ) {
        self.committed
            .set_native_geometry(key, geometry)
            .expect("test utility geometry is valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn path_string(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn native_geometry_intent_keeps_unacknowledged_resize_across_later_move() {
        use crate::settings::SavedWindowPosition;

        let committed = NativeWindowGeometry::new(
            SavedWindowPosition::new(-400.0, 100.0).unwrap(),
            PreferredWindowSize::new(700.0, 500.0).unwrap(),
            1.0,
        )
        .unwrap();
        let resize_rect = PhysicalWindowRect {
            left: -500,
            top: 100,
            right: 420,
            bottom: 800,
        };
        let resized = UtilityNativeGeometrySample {
            position: resize_rect,
            position_scale: 1.0,
            position_sequence: 1,
            size: Some(resize_rect),
            size_sequence: Some(1),
        };
        let after_resize = NativeGeometryIntent::from_committed(Some(committed))
            .followed_by(resized, None)
            .expect("the resize is newer than the acknowledged geometry");
        let clamped_move = UtilityNativeGeometrySample {
            position: PhysicalWindowRect {
                left: -250,
                top: 180,
                right: 250,
                bottom: 480,
            },
            position_scale: 1.0,
            position_sequence: 2,
            size: None,
            size_sequence: None,
        };
        let after_move = after_resize
            .followed_by(clamped_move, None)
            .expect("the later move updates position while retaining resize intent");
        let geometry = after_move.geometry().unwrap();

        assert_eq!(geometry.position.x, -250.0);
        assert_eq!(
            geometry.size,
            PreferredWindowSize::new(920.0, 700.0).unwrap()
        );
        assert!(after_move.followed_by(clamped_move, None).is_none());
    }

    #[test]
    fn utility_size_store_returns_exact_revision_and_publishes_acknowledgement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let path = path_string(&path);
        Settings::default().save(&path).unwrap();
        let workers = SettingsSaveWorkerRegistry::default();
        let key = UtilityToolKey::parse("hash-generator").unwrap();
        let size = PreferredWindowSize::new(720.0, 480.0).unwrap();
        let mut store = UtilitySizeStore::new(path, workers.clone(), UtilityDialogSizes::default());

        let revision = store.set_preferred_size(key, size).unwrap();
        assert_eq!(revision, UtilitySizeRevision(1));
        assert_eq!(store.status(key).pending_revision, Some(revision));
        workers.join_all();

        let poll = store.poll().unwrap();
        assert_eq!(poll.accepted.len(), 1);
        assert_eq!(poll.accepted[0].acknowledgement.revision, revision);
        assert_eq!(
            poll.accepted[0].acknowledgement.preference,
            UtilitySizePreference::Preferred(size)
        );
        assert_eq!(store.committed_sizes().preferred(key), Some(size));
        assert_eq!(store.status(key).acknowledged_revision, Some(revision));
        assert_eq!(store.status(key).pending_revision, None);
    }
}
