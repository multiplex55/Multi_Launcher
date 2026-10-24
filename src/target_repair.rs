//! Guarded repair of persisted filesystem targets and their active references.
//!
//! This module is the persistence owner for one narrowly scoped multi-store
//! operation. It deliberately exposes no UI or path-search behavior.

use crate::actions::Action;
use crate::common::atomic_file::save_atomic;
use crate::common::persistence::{LoadState, read_bytes};
use crate::hotfolder::HotFolderRulesStore;
use crate::mouse_gestures::db::{BindingKind, GestureDb};
use crate::platform::app_data::AppDataRoot;
use crate::plugins::fav::FavEntry;
use crate::plugins::folders::FolderEntry;
use crate::radial::store::RadialStore;
use crate::usage::{self, UsageEntry};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const JOURNAL_FILE: &str = "target_repair_journal.json";
const JOURNAL_VERSION: u32 = 1;
const MAX_REPAIR_STORE_BYTES: usize = 16 * 1024 * 1024;
const MAX_REPAIR_JOURNAL_BYTES: usize = 96 * 1024 * 1024;
const REPAIR_QUEUE_CAPACITY: usize = 8;

static REPAIR_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static NEXT_REPAIR_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub struct UsageHandoff {
    state: Arc<Mutex<UsageHandoffState>>,
}

#[derive(Default)]
struct UsageHandoffState {
    pending: Option<PendingUsageHandoff>,
}

struct PendingUsageHandoff {
    operation_id: u64,
    old: String,
    new: String,
    usage_path: PathBuf,
    deltas: HashMap<String, u32>,
    outcome: Option<UsageHandoffOutcome>,
    conflict_reported: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UsageHandoffOutcome {
    Committed,
    Restored,
    RecoveryConflict,
}

struct UsageOperation {
    handoff: UsageHandoff,
    operation_id: u64,
}

impl UsageHandoff {
    fn begin(
        &self,
        operation_id: u64,
        old: &str,
        new: &str,
        usage_path: PathBuf,
        generation: &AtomicU64,
        expected_generation: u64,
        deferred: &Mutex<HashMap<String, u32>>,
    ) -> Result<UsageOperation, RepairFailure> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.pending.is_some() {
            return Err(failure(
                RepairFailureKind::QueueFull,
                "A previous repair is still reconciling usage; wait before starting another.",
            ));
        }
        if generation.load(Ordering::Acquire) != expected_generation
            || !deferred
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        {
            return Err(failure(
                RepairFailureKind::DestinationCollision,
                "Usage changed while repair was being prepared; refresh and retry.",
            ));
        }
        state.pending = Some(PendingUsageHandoff {
            operation_id,
            old: old.to_owned(),
            new: new.to_owned(),
            usage_path: normalize_usage_path(usage_path),
            deltas: HashMap::new(),
            outcome: None,
            conflict_reported: false,
        });
        Ok(UsageOperation {
            handoff: self.clone(),
            operation_id,
        })
    }

    fn has_pending(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .is_some()
    }

    fn pending_blocks_path(&self, path: &Path) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.pending.as_ref().is_some_and(|pending| {
            pending.outcome == Some(UsageHandoffOutcome::RecoveryConflict)
                && pending.usage_path == normalize_usage_path(path.to_path_buf())
        })
    }

    fn finish(&self, operation_id: u64, outcome: UsageHandoffOutcome) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(pending) = state
            .pending
            .as_mut()
            .filter(|pending| pending.operation_id == operation_id)
        {
            pending.outcome = Some(outcome);
        }
    }

    fn finish_if_unresolved(&self, operation_id: u64, outcome: UsageHandoffOutcome) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(pending) = state
            .pending
            .as_mut()
            .filter(|pending| pending.operation_id == operation_id)
            && pending.outcome.is_none()
        {
            pending.outcome = Some(outcome);
        }
    }

    fn apply_ready(
        &self,
        usage: &mut HashMap<String, u32>,
        deferred: &Mutex<HashMap<String, u32>>,
    ) -> UsageHandoffApply {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut changed = false;
        let mut conflict = false;
        let Some(pending) = state.pending.as_mut() else {
            changed |= apply_usage_deltas(
                usage,
                std::mem::take(
                    &mut *deferred
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                ),
            );
            return UsageHandoffApply { changed, conflict };
        };
        match pending.outcome {
            None => return UsageHandoffApply { changed, conflict },
            Some(UsageHandoffOutcome::RecoveryConflict) => {
                conflict = !pending.conflict_reported;
                pending.conflict_reported = true;
                return UsageHandoffApply { changed, conflict };
            }
            Some(UsageHandoffOutcome::Committed) => {
                if usage.contains_key(&pending.new) {
                    pending.outcome = Some(UsageHandoffOutcome::RecoveryConflict);
                    pending.conflict_reported = true;
                    return UsageHandoffApply {
                        changed,
                        conflict: true,
                    };
                }
                if let Some(count) = usage.remove(&pending.old) {
                    usage.insert(pending.new.clone(), count);
                    changed = true;
                }
                let deltas = std::mem::take(&mut pending.deltas);
                let mut rekeyed = HashMap::new();
                for (path, count) in deltas {
                    let destination = if path == pending.old || path == pending.new {
                        pending.new.clone()
                    } else {
                        path
                    };
                    let entry = rekeyed.entry(destination).or_insert(0_u32);
                    *entry = entry.saturating_add(count);
                }
                changed |= apply_usage_deltas(usage, rekeyed);
            }
            Some(UsageHandoffOutcome::Restored) => {
                changed |= apply_usage_deltas(usage, std::mem::take(&mut pending.deltas));
            }
        }
        state.pending = None;
        changed |= apply_usage_deltas(
            usage,
            std::mem::take(
                &mut *deferred
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            ),
        );
        UsageHandoffApply { changed, conflict }
    }
}

#[derive(Default)]
pub(crate) struct UsageHandoffApply {
    pub(crate) changed: bool,
    pub(crate) conflict: bool,
}

fn apply_usage_deltas(usage: &mut HashMap<String, u32>, deltas: HashMap<String, u32>) -> bool {
    let changed = !deltas.is_empty();
    for (path, count) in deltas {
        let entry = usage.entry(path).or_insert(0);
        *entry = entry.saturating_add(count);
    }
    changed
}

fn normalize_usage_path(path: PathBuf) -> PathBuf {
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

impl UsageOperation {
    fn operation_id(&self) -> u64 {
        self.operation_id
    }

    fn mark_committed(&self) {
        self.handoff
            .finish(self.operation_id, UsageHandoffOutcome::Committed);
    }

    fn mark_recovery_conflict(&self) {
        self.handoff
            .finish(self.operation_id, UsageHandoffOutcome::RecoveryConflict);
    }

    fn mark_restored(&self) {
        self.handoff
            .finish(self.operation_id, UsageHandoffOutcome::Restored);
    }
}

impl Drop for UsageOperation {
    fn drop(&mut self) {
        self.handoff
            .finish_if_unresolved(self.operation_id, UsageHandoffOutcome::Restored);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilesystemTargetKind {
    File,
    Folder,
}

/// A missing custom action no longer reveals whether its former path was a
/// file or directory. Callers must provide the replacement policy captured
/// from the explicit repair surface instead of guessing from its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionTargetKind {
    File,
    FileOrFolder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetInspection {
    Missing,
    Available(FilesystemTargetKind),
    UnknownUnavailable,
    NotFilesystem,
}

#[derive(Clone)]
pub enum SelectedTarget {
    Action {
        original: Action,
        index: usize,
        actions_version: u64,
        allowed_kind: ActionTargetKind,
    },
    Folder {
        original: FolderEntry,
        index: usize,
        folders_version: u64,
    },
}

impl fmt::Debug for SelectedTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Action { index, .. } => formatter
                .debug_struct("SelectedTarget::Action")
                .field("index", index)
                .field("record", &"[redacted]")
                .finish(),
            Self::Folder { index, .. } => formatter
                .debug_struct("SelectedTarget::Folder")
                .field("index", index)
                .field("record", &"[redacted]")
                .finish(),
        }
    }
}

impl SelectedTarget {
    pub fn action(original: Action, index: usize) -> Self {
        Self::action_with_kind(original, index, ActionTargetKind::File)
    }

    pub fn action_with_kind(
        original: Action,
        index: usize,
        allowed_kind: ActionTargetKind,
    ) -> Self {
        Self::Action {
            original,
            index,
            actions_version: crate::actions::actions_version(),
            allowed_kind,
        }
    }

    pub fn folder(original: FolderEntry, index: usize) -> Self {
        Self::folder_at_version(original, index, crate::plugins::folders::folders_version())
    }

    pub(crate) fn folder_at_version(
        original: FolderEntry,
        index: usize,
        folders_version: u64,
    ) -> Self {
        Self::Folder {
            original,
            index,
            folders_version,
        }
    }

    pub fn original_path(&self) -> &str {
        self.old_path()
    }

    pub fn is_folder_entry(&self) -> bool {
        matches!(self, Self::Folder { .. })
    }

    pub fn same_capture(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Action {
                    original: left,
                    index: left_index,
                    actions_version: left_version,
                    allowed_kind: left_kind,
                },
                Self::Action {
                    original: right,
                    index: right_index,
                    actions_version: right_version,
                    allowed_kind: right_kind,
                },
            ) => {
                left == right
                    && left_index == right_index
                    && left_version == right_version
                    && left_kind == right_kind
            }
            (
                Self::Folder {
                    original: left,
                    index: left_index,
                    folders_version: left_version,
                },
                Self::Folder {
                    original: right,
                    index: right_index,
                    folders_version: right_version,
                },
            ) => left == right && left_index == right_index && left_version == right_version,
            _ => false,
        }
    }

    fn allows_replacement_kind(&self, kind: FilesystemTargetKind) -> bool {
        match self {
            Self::Action { allowed_kind, .. } => match allowed_kind {
                ActionTargetKind::File => kind == FilesystemTargetKind::File,
                ActionTargetKind::FileOrFolder => true,
            },
            Self::Folder { .. } => kind == FilesystemTargetKind::Folder,
        }
    }

    fn old_path(&self) -> &str {
        match self {
            Self::Action { original, .. } => &original.action,
            Self::Folder { original, .. } => &original.path,
        }
    }
}

#[derive(Clone)]
pub struct RepairRequest {
    pub selected: SelectedTarget,
    pub replacement: PathBuf,
    expected_replacement_kind: Option<FilesystemTargetKind>,
    presentation_token: Option<u64>,
    live_usage: Option<LiveUsageSnapshot>,
}

/// A point-in-time view of the app-owned usage map, paired with a short
/// commit barrier. The repair worker checks the generation while holding the
/// barrier immediately before its durable commit marker.
#[derive(Clone)]
pub struct LiveUsageSnapshot {
    counts: HashMap<String, u32>,
    observed_generation: u64,
    generation: Arc<AtomicU64>,
    mutation_barrier: Arc<Mutex<()>>,
    deferred: Arc<Mutex<HashMap<String, u32>>>,
    handoff: UsageHandoff,
}

impl LiveUsageSnapshot {
    pub fn capture(
        counts: &HashMap<String, u32>,
        generation: Arc<AtomicU64>,
        mutation_barrier: Arc<Mutex<()>>,
        deferred: Arc<Mutex<HashMap<String, u32>>>,
        handoff: UsageHandoff,
    ) -> Result<Self, RepairFailure> {
        let handoff_state = match handoff.state.try_lock() {
            Ok(state) => state,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(failure(
                    RepairFailureKind::QueueFull,
                    "Usage is being reconciled; wait before starting another repair.",
                ));
            }
        };
        if handoff_state.pending.is_some() {
            return Err(failure(
                RepairFailureKind::QueueFull,
                "Usage is being reconciled; wait before starting another repair.",
            ));
        }
        let _usage_guard = match mutation_barrier.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(failure(
                    RepairFailureKind::QueueFull,
                    "Usage is being reconciled; wait before starting another repair.",
                ));
            }
        };
        if !deferred
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
        {
            return Err(failure(
                RepairFailureKind::QueueFull,
                "Usage is still being reconciled; wait before starting another repair.",
            ));
        }
        let counts = counts.clone();
        let observed_generation = generation.load(Ordering::Acquire);
        drop(_usage_guard);
        drop(handoff_state);
        let snapshot = Self {
            counts,
            observed_generation,
            generation,
            mutation_barrier,
            deferred,
            handoff,
        };
        Ok(snapshot)
    }
}

/// Records one app-owned usage increment without ever making an input/UI
/// caller wait for target-repair journal I/O. Increments arriving during the
/// commit barrier are replayed after the corresponding receipt is applied.
pub(crate) fn record_usage_delta(
    path: &str,
    usage: &mut HashMap<String, u32>,
    deferred: &Mutex<HashMap<String, u32>>,
    generation: &AtomicU64,
    barrier: &Mutex<()>,
    handoff: &UsageHandoff,
) {
    let mut handoff_state = handoff
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(pending) = handoff_state.pending.as_mut() {
        let count = pending.deltas.entry(path.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
        return;
    }
    let usage_guard = match barrier.try_lock() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::Poisoned(error)) => Some(error.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    };
    if let Some(_usage_guard) = usage_guard {
        let count = usage.entry(path.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
        generation.fetch_add(1, Ordering::AcqRel);
    } else {
        let mut deferred = deferred
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = deferred.entry(path.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
        generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl fmt::Debug for LiveUsageSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LiveUsageSnapshot")
            .field("entry_count", &self.counts.len())
            .field("observed_generation", &self.observed_generation)
            .finish()
    }
}

impl RepairRequest {
    pub fn new(selected: SelectedTarget, replacement: PathBuf) -> Self {
        Self {
            selected,
            replacement,
            expected_replacement_kind: None,
            presentation_token: None,
            live_usage: None,
        }
    }

    pub fn with_expected_replacement_kind(mut self, kind: FilesystemTargetKind) -> Self {
        self.expected_replacement_kind = Some(kind);
        self
    }

    pub fn with_presentation_token(mut self, token: u64) -> Self {
        self.presentation_token = Some(token);
        self
    }

    pub(crate) fn presentation_token(&self) -> Option<u64> {
        self.presentation_token
    }

    pub fn with_live_usage(mut self, snapshot: LiveUsageSnapshot) -> Self {
        self.live_usage = Some(snapshot);
        self
    }
}

impl fmt::Debug for RepairRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepairRequest")
            .field("selected", &self.selected)
            .field("replacement", &"[redacted]")
            .field("expected_replacement_kind", &self.expected_replacement_kind)
            .field("presentation_token", &self.presentation_token)
            .field("live_usage", &self.live_usage.is_some())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairFailureKind {
    NotFilesystem,
    SourceAvailable,
    SourceUnavailable,
    ReplacementUnavailable,
    ReplacementWrongKind,
    StaleSelection,
    AmbiguousSelection,
    DestinationCollision,
    SharedUsageKey,
    ActiveRadialReference,
    HotFolderAuthorization,
    OwnerUnavailable,
    StoreMalformed,
    StoreUnavailable,
    RecoveryConflict,
    PersistFailed,
    QueueFull,
    WorkerUnavailable,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RepairFailure {
    pub kind: RepairFailureKind,
    pub message: &'static str,
}

impl fmt::Debug for RepairFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepairFailure")
            .field("kind", &self.kind)
            .field("message", &self.message)
            .finish()
    }
}

impl fmt::Display for RepairFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for RepairFailure {}

#[derive(Clone)]
pub struct RepairCommit {
    pub operation_id: u64,
    pub updated_action: Option<Action>,
    pub updated_folder: Option<FolderEntry>,
    /// Complete custom prefix from the committed Actions store. This lets the
    /// GUI publish unrelated rows that were present on disk when the repair
    /// transaction ran instead of reconstructing from a stale app cache.
    pub custom_actions: Option<Vec<Action>>,
    /// Actions owner revision produced by this commit. The GUI refuses to
    /// publish this snapshot after any newer catalog revision is visible.
    pub actions_version: Option<u64>,
    pub usage_rekey: Option<(String, String)>,
}

impl fmt::Debug for RepairCommit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepairCommit")
            .field("operation_id", &self.operation_id)
            .field("updated_action", &self.updated_action.is_some())
            .field("updated_folder", &self.updated_folder.is_some())
            .field("custom_actions", &self.custom_actions.is_some())
            .field("actions_version", &self.actions_version)
            .field("usage_rekey", &self.usage_rekey.is_some())
            .finish()
    }
}

#[derive(Clone)]
pub struct TargetRepairCoordinator {
    root: PathBuf,
    radial: Arc<RadialStore>,
    hotfolder: HotFolderRulesStore,
}

impl TargetRepairCoordinator {
    pub fn new(
        root: impl Into<PathBuf>,
        radial: Arc<RadialStore>,
        hotfolder: HotFolderRulesStore,
    ) -> Self {
        Self {
            root: root.into(),
            radial,
            hotfolder,
        }
    }

    pub fn repair(&self, request: RepairRequest) -> Result<RepairCommit, RepairFailure> {
        let source = request.selected.old_path();
        match inspect_target(source) {
            TargetInspection::Missing => {}
            TargetInspection::Available(_) => {
                return Err(failure(
                    RepairFailureKind::SourceAvailable,
                    "The saved target is still available; review it before repairing.",
                ));
            }
            TargetInspection::UnknownUnavailable => {
                return Err(failure(
                    RepairFailureKind::SourceUnavailable,
                    "The saved target could not be inspected; no repair was written.",
                ));
            }
            TargetInspection::NotFilesystem => {
                return Err(failure(
                    RepairFailureKind::NotFilesystem,
                    "This saved target is not a filesystem path.",
                ));
            }
        }
        validate_replacement(
            &request.replacement,
            &request.selected,
            request.expected_replacement_kind,
        )?;
        let new_path = request
            .replacement
            .to_str()
            .ok_or_else(|| {
                failure(
                    RepairFailureKind::ReplacementUnavailable,
                    "The replacement path cannot be represented safely.",
                )
            })?
            .to_owned();

        let operation_id = NEXT_REPAIR_ID.fetch_add(1, Ordering::Relaxed);
        let usage_operation = request
            .live_usage
            .as_ref()
            .map(|snapshot| {
                snapshot.handoff.begin(
                    operation_id,
                    source,
                    &new_path,
                    self.root.join(usage::USAGE_FILE),
                    &snapshot.generation,
                    snapshot.observed_generation,
                    &snapshot.deferred,
                )
            })
            .transpose()?;

        let _repair_transaction = REPAIR_TRANSACTION
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let paths = store_paths(&self.root);
        let mut watch_fence = match crate::common::repair_watch::RepairWatchFence::new(
            repair_fence_paths(&self.root),
        ) {
            Ok(fence) => fence,
            Err(_) => {
                if let Some(operation) = usage_operation.as_ref() {
                    operation.mark_recovery_conflict();
                }
                return Err(recovery_conflict());
            }
        };
        // Publication APIs take UTF-8 paths. Validate them before any durable
        // change so a path encoding failure cannot strand committed files
        // without their in-memory owner publication.
        let folders_path = store_path_string(&paths, StoreFile::Folders)?.to_owned();
        let favorites_path = store_path_string(&paths, StoreFile::Favorites)?.to_owned();
        match fs::metadata(self.root.join(JOURNAL_FILE)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) | Err(_) => {
                watch_fence.mark_transaction_started();
                if let Some(operation) = usage_operation.as_ref() {
                    operation.mark_recovery_conflict();
                }
                return Err(recovery_conflict());
            }
        }

        // Refresh the same worker-owned store before taking the fixed owner
        // lock sequence. Disabled Hot Folder still has a store handle and its
        // persisted authorization records remain part of preflight.
        let report = self.hotfolder.load_for_target_repair();
        if !matches!(
            report.status,
            crate::hotfolder::HotFolderLoadStatus::Missing
                | crate::hotfolder::HotFolderLoadStatus::Loaded
                | crate::hotfolder::HotFolderLoadStatus::LoadedWithQuarantinedRows
        ) {
            return Err(failure(
                RepairFailureKind::OwnerUnavailable,
                "Hot Folder authorization state is unavailable; no repair was written.",
            ));
        }

        // Global owner lock order: actions, folders, usage, pins, favorites,
        // gestures, radial, Hot Folder. Each owner mutation takes only its own
        // lock, so no reverse cross-owner order exists.
        let _actions_guard = crate::actions::transaction_guard();
        let _folders_guard = crate::plugins::folders::folders_transaction_guard();
        let _usage_guard = usage::transaction_guard();
        let _pins_guard = crate::history::pins_transaction_guard();
        let _favorites_guard = crate::plugins::fav::fav_transaction_guard();
        let _gestures_guard = crate::mouse_gestures::db::gestures_transaction_guard();
        let radial_guard = self.radial.target_repair_guard().map_err(|_| {
            failure(
                RepairFailureKind::OwnerUnavailable,
                "Radial references could not be verified; no repair was written.",
            )
        })?;
        let hotfolder_guard = self.hotfolder.repair_guard();
        if !hotfolder_guard.is_authoritative(self.hotfolder.path()) {
            return Err(failure(
                RepairFailureKind::OwnerUnavailable,
                "Hot Folder authorization state changed during preflight; no repair was written.",
            ));
        }

        let mut documents = StoreDocuments::load(&paths)?;
        let target = validate_selected(&request.selected, &mut documents)?;
        match target {
            TargetData::Action { original } => {
                if radial_guard.has_action_reference(&original.action, original.args.as_deref()) {
                    return Err(failure(
                        RepairFailureKind::ActiveRadialReference,
                        "Radial has a persisted reference to this target; repair was refused.",
                    ));
                }
                if hotfolder_guard
                    .has_auth_bound_reference(&original.action, original.args.as_deref())
                {
                    return Err(failure(
                        RepairFailureKind::HotFolderAuthorization,
                        "Hot Folder has an authorization-bound reference to this target; repair was refused.",
                    ));
                }
                ensure_action_destination_available(&documents.actions, &original, &new_path)?;
                let (updated, usage_rekey) = documents.repair_action(
                    &original,
                    &new_path,
                    request.live_usage.as_ref().map(|snapshot| &snapshot.counts),
                )?;
                let custom_actions = documents.actions.clone();
                let changes = documents.changes(&paths)?;
                watch_fence.mark_transaction_started();
                let committed = match persist_transaction_inner(
                    &self.root,
                    &changes,
                    read_store,
                    write_store,
                    request.live_usage.as_ref(),
                    operation_id,
                    usage_operation.as_ref(),
                    Some(custom_actions),
                ) {
                    Ok(committed) => {
                        watch_fence.resolve();
                        committed
                    }
                    Err(error) => {
                        if error.kind == RepairFailureKind::RecoveryConflict
                            && let Some(operation) = usage_operation.as_ref()
                        {
                            operation.mark_recovery_conflict();
                        }
                        if error.kind != RepairFailureKind::RecoveryConflict {
                            watch_fence.resolve();
                        }
                        return Err(error);
                    }
                };
                crate::actions::bump_actions_version();
                let actions_version = crate::actions::actions_version();
                if let Some(favorites) = committed.favorites.as_deref() {
                    crate::plugins::fav::publish_repaired_favs(&favorites_path, favorites);
                }
                drop(hotfolder_guard);
                drop(radial_guard);
                drop(_gestures_guard);
                drop(_favorites_guard);
                drop(_pins_guard);
                drop(_usage_guard);
                drop(_folders_guard);
                drop(_actions_guard);
                drop(_repair_transaction);
                drop(watch_fence);
                Ok(RepairCommit {
                    operation_id: committed.operation_id,
                    updated_action: Some(updated),
                    updated_folder: None,
                    custom_actions: committed.custom_actions,
                    actions_version: Some(actions_version),
                    usage_rekey,
                })
            }
            TargetData::Folder { original } => {
                if radial_guard.has_folder_reference(&original.path) {
                    return Err(failure(
                        RepairFailureKind::ActiveRadialReference,
                        "Radial has a persisted reference to this folder; repair was refused.",
                    ));
                }
                if hotfolder_guard.has_folder_reference(&original.path) {
                    return Err(failure(
                        RepairFailureKind::HotFolderAuthorization,
                        "Hot Folder has an authorization-bound reference to this folder; repair was refused.",
                    ));
                }
                ensure_folder_destination_available(&documents.folders, &original, &new_path)?;
                let (updated, usage_rekey) = documents.repair_folder(
                    &original,
                    &new_path,
                    request.live_usage.as_ref().map(|snapshot| &snapshot.counts),
                )?;
                let changes = documents.changes(&paths)?;
                watch_fence.mark_transaction_started();
                let committed = match persist_transaction_inner(
                    &self.root,
                    &changes,
                    read_store,
                    write_store,
                    request.live_usage.as_ref(),
                    operation_id,
                    usage_operation.as_ref(),
                    None,
                ) {
                    Ok(committed) => {
                        watch_fence.resolve();
                        committed
                    }
                    Err(error) => {
                        if error.kind == RepairFailureKind::RecoveryConflict
                            && let Some(operation) = usage_operation.as_ref()
                        {
                            operation.mark_recovery_conflict();
                        }
                        if error.kind != RepairFailureKind::RecoveryConflict {
                            watch_fence.resolve();
                        }
                        return Err(error);
                    }
                };
                crate::plugins::folders::publish_repaired_folders(
                    &folders_path,
                    &committed.folders,
                );
                if let Some(favorites) = committed.favorites.as_deref() {
                    crate::plugins::fav::publish_repaired_favs(&favorites_path, favorites);
                }
                drop(hotfolder_guard);
                drop(radial_guard);
                drop(_gestures_guard);
                drop(_favorites_guard);
                drop(_pins_guard);
                drop(_usage_guard);
                drop(_folders_guard);
                drop(_actions_guard);
                drop(_repair_transaction);
                drop(watch_fence);
                Ok(RepairCommit {
                    operation_id: committed.operation_id,
                    updated_action: None,
                    updated_folder: Some(updated),
                    custom_actions: None,
                    actions_version: None,
                    usage_rekey,
                })
            }
        }
    }
}

pub fn inspect_target(path: &str) -> TargetInspection {
    let path = Path::new(path);
    if !path.is_absolute() {
        return TargetInspection::NotFilesystem;
    }
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            TargetInspection::Available(FilesystemTargetKind::File)
        }
        Ok(metadata) if metadata.is_dir() => {
            TargetInspection::Available(FilesystemTargetKind::Folder)
        }
        Ok(_) => TargetInspection::UnknownUnavailable,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => TargetInspection::Missing,
        Err(_) => TargetInspection::UnknownUnavailable,
    }
}

fn validate_replacement(
    replacement: &Path,
    selected: &SelectedTarget,
    expected_kind: Option<FilesystemTargetKind>,
) -> Result<(), RepairFailure> {
    let metadata = fs::metadata(replacement).map_err(|_| {
        failure(
            RepairFailureKind::ReplacementUnavailable,
            "The replacement target is missing or unavailable.",
        )
    })?;
    let actual_kind = if metadata.is_file() {
        Some(FilesystemTargetKind::File)
    } else if metadata.is_dir() {
        Some(FilesystemTargetKind::Folder)
    } else {
        None
    };
    if actual_kind.is_none_or(|kind| {
        !selected.allows_replacement_kind(kind)
            || expected_kind.is_some_and(|expected| kind != expected)
    }) {
        return Err(failure(
            RepairFailureKind::ReplacementWrongKind,
            "The replacement must be an allowed file or folder target.",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoreFile {
    Actions,
    Folders,
    Pins,
    Favorites,
    Gestures,
    Usage,
}

impl StoreFile {
    fn file_name(self) -> &'static str {
        match self {
            Self::Actions => "actions.json",
            Self::Folders => crate::plugins::folders::FOLDERS_FILE,
            Self::Pins => crate::history::HISTORY_PINS_FILE,
            Self::Favorites => crate::plugins::fav::FAV_FILE,
            Self::Gestures => crate::mouse_gestures::db::GESTURES_FILE,
            Self::Usage => crate::usage::USAGE_FILE,
        }
    }
}

fn store_paths(root: &Path) -> Vec<(StoreFile, PathBuf)> {
    [
        StoreFile::Actions,
        StoreFile::Folders,
        StoreFile::Pins,
        StoreFile::Favorites,
        StoreFile::Gestures,
        StoreFile::Usage,
    ]
    .into_iter()
    .map(|store| (store, root.join(store.file_name())))
    .collect()
}

fn repair_fence_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = store_paths(root)
        .into_iter()
        .map(|(_, path)| path)
        .collect::<Vec<_>>();
    paths.push(root.join(crate::hotfolder::HOT_FOLDER_RULES_FILE));
    paths
}

fn store_path_string(
    paths: &[(StoreFile, PathBuf)],
    store: StoreFile,
) -> Result<&str, RepairFailure> {
    paths
        .iter()
        .find(|(candidate, _)| *candidate == store)
        .and_then(|(_, path)| path.to_str())
        .ok_or_else(owner_unavailable)
}

struct StoreDocuments {
    before: HashMap<StoreFile, Option<Vec<u8>>>,
    values: HashMap<StoreFile, Value>,
    actions: Vec<Action>,
    folders: Vec<FolderEntry>,
    pins: Vec<crate::history::HistoryPin>,
    favorites: Vec<FavEntry>,
    usage: Vec<UsageEntry>,
    gestures: GestureDb,
    gestures_current_schema: bool,
    dirty: std::collections::HashSet<StoreFile>,
}

struct CommittedStores {
    folders: Vec<FolderEntry>,
    favorites: Option<Vec<FavEntry>>,
}

struct UsageRekeyPlan {
    index: Option<usize>,
    old: String,
    new: String,
    count: u32,
}

impl StoreDocuments {
    fn load(paths: &[(StoreFile, PathBuf)]) -> Result<Self, RepairFailure> {
        let mut before = HashMap::new();
        let mut values = HashMap::new();
        for (store, path) in paths {
            let bytes = read_store(path)?;
            let value = match bytes.as_deref() {
                None => Value::Array(Vec::new()),
                Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => {
                    Value::Array(Vec::new())
                }
                Some(bytes) => serde_json::from_slice(bytes).map_err(|_| {
                    failure(
                        RepairFailureKind::StoreMalformed,
                        "A related saved-target store is malformed; no repair was written.",
                    )
                })?,
            };
            before.insert(*store, bytes);
            values.insert(*store, value);
        }
        let actions = decode_vec(&values, StoreFile::Actions)?;
        let folders = decode_vec(&values, StoreFile::Folders)?;
        let favorites = decode_vec(&values, StoreFile::Favorites)?;
        let usage = decode_vec(&values, StoreFile::Usage)?;
        let pins: Vec<crate::history::HistoryPin> = decode_vec(&values, StoreFile::Pins)?;
        let (gestures, gestures_current_schema) =
            match before.get(&StoreFile::Gestures).and_then(Option::as_deref) {
                None => (GestureDb::default(), true),
                Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => {
                    (GestureDb::default(), true)
                }
                Some(bytes) => {
                    let path = paths
                        .iter()
                        .find(|(store, _)| *store == StoreFile::Gestures)
                        .map(|(_, path)| path.as_path())
                        .ok_or_else(owner_unavailable)?;
                    let gestures = crate::mouse_gestures::db::decode_gestures(path, bytes)
                        .map_err(|_| {
                            failure(
                                RepairFailureKind::StoreMalformed,
                                "The gesture store is malformed; no repair was written.",
                            )
                        })?;
                    let current = values
                        .get(&StoreFile::Gestures)
                        .and_then(|value| value.get("schema_version"))
                        .and_then(Value::as_u64)
                        == Some(crate::mouse_gestures::db::SCHEMA_VERSION as u64);
                    (gestures, current)
                }
            };
        Ok(Self {
            before,
            values,
            actions,
            folders,
            pins,
            favorites,
            usage,
            gestures,
            gestures_current_schema,
            dirty: std::collections::HashSet::new(),
        })
    }

    fn repair_action(
        &mut self,
        original: &Action,
        replacement: &str,
        live_usage: Option<&HashMap<String, u32>>,
    ) -> Result<(Action, Option<(String, String)>), RepairFailure> {
        if self.has_ambiguous_path_associations(&original.action, original.args.as_deref()) {
            return Err(failure(
                RepairFailureKind::AmbiguousSelection,
                "A path-only saved association matches multiple targets; repair was refused.",
            ));
        }
        let usage_plan = self.plan_usage_rekey(&original.action, replacement, live_usage)?;
        let action_index = self
            .actions
            .iter()
            .position(|action| action == original)
            .ok_or_else(stale_selection)?;
        let mut updated = original.clone();
        updated.action = replacement.to_owned();
        let action_values = array_mut(&mut self.values, StoreFile::Actions)?;
        set_string_field(action_values.get_mut(action_index), "action", replacement)?;
        self.actions[action_index] = updated.clone();
        self.dirty.insert(StoreFile::Actions);

        let pins_values = array_mut(&mut self.values, StoreFile::Pins)?;
        let mut pins_changed = false;
        for (index, pin) in self.pins.iter_mut().enumerate() {
            if pin.matches_id(&original.action, original.args.as_deref()) {
                set_string_field(pins_values.get_mut(index), "action_id", replacement)?;
                pin.action_id = replacement.to_owned();
                pins_changed = true;
            }
        }
        if pins_changed {
            self.dirty.insert(StoreFile::Pins);
        }
        let fav_values = array_mut(&mut self.values, StoreFile::Favorites)?;
        let mut favorites_changed = false;
        for (index, fav) in self.favorites.iter_mut().enumerate() {
            if fav.action == original.action && fav.args == original.args {
                fav.action = replacement.to_owned();
                set_string_field(fav_values.get_mut(index), "action", replacement)?;
                favorites_changed = true;
            }
        }
        if favorites_changed {
            self.dirty.insert(StoreFile::Favorites);
        }
        if self.repair_gesture_bindings(&original.action, original.args.as_deref(), replacement)? {
            self.dirty.insert(StoreFile::Gestures);
        }
        let usage_rekey = self.apply_usage_rekey(usage_plan)?;
        Ok((updated, usage_rekey))
    }

    fn repair_folder(
        &mut self,
        original: &FolderEntry,
        replacement: &str,
        live_usage: Option<&HashMap<String, u32>>,
    ) -> Result<(FolderEntry, Option<(String, String)>), RepairFailure> {
        if self.has_ambiguous_path_associations(&original.path, None) {
            return Err(failure(
                RepairFailureKind::AmbiguousSelection,
                "A path-only saved association matches multiple targets; repair was refused.",
            ));
        }
        let usage_plan = self.plan_usage_rekey(&original.path, replacement, live_usage)?;
        let folder_index = self
            .folders
            .iter()
            .position(|folder| folder == original)
            .ok_or_else(stale_selection)?;
        let mut updated = original.clone();
        updated.path = replacement.to_owned();
        let folder_values = array_mut(&mut self.values, StoreFile::Folders)?;
        set_string_field(folder_values.get_mut(folder_index), "path", replacement)?;
        self.dirty.insert(StoreFile::Folders);

        let pins_values = array_mut(&mut self.values, StoreFile::Pins)?;
        let mut pins_changed = false;
        for (index, pin) in self.pins.iter_mut().enumerate() {
            if pin.matches_id(&original.path, None) {
                set_string_field(pins_values.get_mut(index), "action_id", replacement)?;
                pin.action_id = replacement.to_owned();
                pins_changed = true;
            }
        }
        if pins_changed {
            self.dirty.insert(StoreFile::Pins);
        }
        let fav_values = array_mut(&mut self.values, StoreFile::Favorites)?;
        let mut favorites_changed = false;
        for (index, fav) in self.favorites.iter_mut().enumerate() {
            if fav.action == original.path && fav.args.is_none() {
                fav.action = replacement.to_owned();
                set_string_field(fav_values.get_mut(index), "action", replacement)?;
                favorites_changed = true;
            }
        }
        if favorites_changed {
            self.dirty.insert(StoreFile::Favorites);
        }
        if self.repair_gesture_bindings(&original.path, None, replacement)? {
            self.dirty.insert(StoreFile::Gestures);
        }
        let usage_rekey = self.apply_usage_rekey(usage_plan)?;
        Ok((updated, usage_rekey))
    }

    fn repair_gesture_bindings(
        &mut self,
        old: &str,
        args: Option<&str>,
        replacement: &str,
    ) -> Result<bool, RepairFailure> {
        let matches =
            self.gestures
                .gestures
                .iter()
                .enumerate()
                .flat_map(|(gesture_index, gesture)| {
                    gesture.bindings.iter().enumerate().filter_map(
                        move |(binding_index, binding)| {
                            (binding.kind == BindingKind::Execute
                                && binding.action == old
                                && binding.args.as_deref() == args)
                                .then_some((gesture_index, binding_index))
                        },
                    )
                })
                .collect::<Vec<_>>();
        if matches.is_empty() {
            return Ok(false);
        }
        if !self.gestures_current_schema {
            return Err(failure(
                RepairFailureKind::StoreMalformed,
                "The gesture store needs migration before its active references can be repaired.",
            ));
        }
        let root = self
            .values
            .get_mut(&StoreFile::Gestures)
            .ok_or_else(owner_unavailable)?;
        let gestures = root
            .get_mut("gestures")
            .and_then(Value::as_array_mut)
            .ok_or_else(owner_unavailable)?;
        for (gesture_index, binding_index) in matches {
            let Some(binding_value) = gestures
                .get_mut(gesture_index)
                .and_then(|gesture| gesture.get_mut("bindings"))
                .and_then(Value::as_array_mut)
                .and_then(|bindings| bindings.get_mut(binding_index))
            else {
                return Err(owner_unavailable());
            };
            set_string_field(Some(binding_value), "action", replacement)?;
            if let Some(binding) = self
                .gestures
                .gestures
                .get_mut(gesture_index)
                .and_then(|gesture| gesture.bindings.get_mut(binding_index))
            {
                binding.action = replacement.to_owned();
            }
        }
        Ok(true)
    }

    fn plan_usage_rekey(
        &self,
        old: &str,
        replacement: &str,
        live_usage: Option<&HashMap<String, u32>>,
    ) -> Result<Option<UsageRekeyPlan>, RepairFailure> {
        let matching_usage = self
            .usage
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.action == old)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let matching_records = self
            .actions
            .iter()
            .filter(|action| action.action == old)
            .count()
            + self
                .folders
                .iter()
                .filter(|folder| folder.path == old)
                .count();
        let live_source_count = live_usage.and_then(|usage| usage.get(old)).copied();
        if matching_usage.is_empty() && live_source_count.is_none() {
            return Ok(None);
        }
        let live_destination_count = live_usage.and_then(|usage| usage.get(replacement));
        if live_destination_count.is_some() {
            return Err(failure(
                RepairFailureKind::DestinationCollision,
                "The replacement already has live usage data; repair was refused.",
            ));
        }
        if matching_records != 1 || matching_usage.len() > 1 {
            return Err(failure(
                RepairFailureKind::SharedUsageKey,
                "Usage is shared by multiple saved targets at this path; repair was refused.",
            ));
        }
        if self.usage.iter().any(|entry| entry.action == replacement) {
            return Err(failure(
                RepairFailureKind::DestinationCollision,
                "The replacement already has usage data; repair was refused.",
            ));
        }

        let Some(count) = live_source_count
            .or_else(|| matching_usage.first().map(|index| self.usage[*index].count))
        else {
            return Ok(None);
        };
        Ok(Some(UsageRekeyPlan {
            index: matching_usage.first().copied(),
            old: old.to_owned(),
            new: replacement.to_owned(),
            count,
        }))
    }

    fn has_ambiguous_path_associations(&self, path: &str, args: Option<&str>) -> bool {
        let owners = self
            .actions
            .iter()
            .filter(|action| action.action == path && action.args.as_deref() == args)
            .count()
            + if args.is_none() {
                self.folders
                    .iter()
                    .filter(|folder| folder.path == path)
                    .count()
            } else {
                0
            };
        if owners < 2 {
            return false;
        }
        self.pins.iter().any(|pin| pin.matches_id(path, args))
            || self
                .favorites
                .iter()
                .any(|favorite| favorite.action == path && favorite.args.as_deref() == args)
            || self.gestures.gestures.iter().any(|gesture| {
                gesture.bindings.iter().any(|binding| {
                    binding.kind == BindingKind::Execute
                        && binding.action == path
                        && binding.args.as_deref() == args
                })
            })
    }

    fn apply_usage_rekey(
        &mut self,
        plan: Option<UsageRekeyPlan>,
    ) -> Result<Option<(String, String)>, RepairFailure> {
        let Some(plan) = plan else {
            return Ok(None);
        };
        let usage_values = array_mut(&mut self.values, StoreFile::Usage)?;
        if let Some(index) = plan.index {
            self.usage[index].action = plan.new.clone();
            self.usage[index].count = plan.count;
            set_string_field(usage_values.get_mut(index), "action", &plan.new)?;
            if let Some(object) = usage_values.get_mut(index).and_then(Value::as_object_mut) {
                object.insert("count".to_owned(), Value::from(plan.count));
            }
        } else {
            let entry = UsageEntry {
                action: plan.new.clone(),
                count: plan.count,
            };
            self.usage.push(entry.clone());
            usage_values.push(serde_json::to_value(entry).map_err(|_| owner_unavailable())?);
        }
        self.dirty.insert(StoreFile::Usage);
        Ok(Some((plan.old, plan.new)))
    }

    fn changes(&self, paths: &[(StoreFile, PathBuf)]) -> Result<Vec<FileChange>, RepairFailure> {
        let mut changes = Vec::new();
        for store in &self.dirty {
            let value = self.values.get(store).ok_or_else(owner_unavailable)?;
            let after = serde_json::to_vec_pretty(value).map_err(|_| {
                failure(
                    RepairFailureKind::StoreMalformed,
                    "A related store could not be prepared safely; no repair was written.",
                )
            })?;
            let before = self.before.get(store).cloned().flatten();
            if before.as_deref() == Some(after.as_slice()) {
                continue;
            }
            if after.len() > MAX_REPAIR_STORE_BYTES {
                return Err(failure(
                    RepairFailureKind::StoreUnavailable,
                    "A related store exceeds the supported repair size.",
                ));
            }
            changes.push(FileChange {
                store: *store,
                path: paths
                    .iter()
                    .find(|(candidate, _)| candidate == store)
                    .map(|(_, path)| path.clone())
                    .ok_or_else(owner_unavailable)?,
                before,
                after,
            });
        }
        changes.sort_by_key(|change| change.store);
        Ok(changes)
    }
}

enum TargetData {
    Action { original: Action },
    Folder { original: FolderEntry },
}

fn validate_selected(
    selected: &SelectedTarget,
    documents: &mut StoreDocuments,
) -> Result<TargetData, RepairFailure> {
    match selected {
        SelectedTarget::Action {
            original,
            index,
            actions_version,
            ..
        } => {
            if crate::actions::actions_version() != *actions_version {
                return Err(stale_selection());
            }
            if documents.actions.get(*index) != Some(original) {
                return Err(stale_selection());
            }
            if documents
                .actions
                .iter()
                .filter(|action| *action == original)
                .count()
                != 1
            {
                return Err(failure(
                    RepairFailureKind::AmbiguousSelection,
                    "The selected saved action is ambiguous; refresh the list before repairing.",
                ));
            }
            Ok(TargetData::Action {
                original: original.clone(),
            })
        }
        SelectedTarget::Folder {
            original,
            index,
            folders_version,
        } => {
            if crate::plugins::folders::folders_version() != *folders_version {
                return Err(stale_selection());
            }
            if documents.folders.get(*index) != Some(original) {
                return Err(stale_selection());
            }
            if documents
                .folders
                .iter()
                .filter(|folder| *folder == original)
                .count()
                != 1
            {
                return Err(failure(
                    RepairFailureKind::AmbiguousSelection,
                    "The selected saved folder is ambiguous; refresh the list before repairing.",
                ));
            }
            Ok(TargetData::Folder {
                original: original.clone(),
            })
        }
    }
}

fn ensure_action_destination_available(
    actions: &[Action],
    original: &Action,
    replacement: &str,
) -> Result<(), RepairFailure> {
    let key = normalized_path_key(replacement);
    if actions
        .iter()
        .any(|action| action != original && normalized_path_key(&action.action) == key)
    {
        return Err(failure(
            RepairFailureKind::DestinationCollision,
            "Another saved action already uses the replacement path.",
        ));
    }
    Ok(())
}

fn ensure_folder_destination_available(
    folders: &[FolderEntry],
    original: &FolderEntry,
    replacement: &str,
) -> Result<(), RepairFailure> {
    let key = normalized_path_key(replacement);
    if folders
        .iter()
        .any(|folder| folder != original && normalized_path_key(&folder.path) == key)
    {
        return Err(failure(
            RepairFailureKind::DestinationCollision,
            "Another saved folder already uses the replacement path.",
        ));
    }
    Ok(())
}

fn normalized_path_key(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

fn read_store(path: &Path) -> Result<Option<Vec<u8>>, RepairFailure> {
    match read_bytes(path).map_err(|_| {
        failure(
            RepairFailureKind::StoreUnavailable,
            "A related saved-target store is unavailable; no repair was written.",
        )
    })? {
        LoadState::Missing => Ok(None),
        LoadState::Empty => Ok(Some(Vec::new())),
        LoadState::Loaded(bytes) if bytes.len() <= MAX_REPAIR_STORE_BYTES => Ok(Some(bytes)),
        LoadState::Loaded(_) => Err(failure(
            RepairFailureKind::StoreUnavailable,
            "A related saved-target store exceeds the supported repair size.",
        )),
    }
}

fn decode_vec<T>(
    values: &HashMap<StoreFile, Value>,
    store: StoreFile,
) -> Result<Vec<T>, RepairFailure>
where
    T: for<'de> Deserialize<'de>,
{
    decode_optional(values, store)
}

fn decode_optional<T>(
    values: &HashMap<StoreFile, Value>,
    store: StoreFile,
) -> Result<T, RepairFailure>
where
    T: for<'de> Deserialize<'de> + Default,
{
    let value = values.get(&store).ok_or_else(owner_unavailable)?;
    if value.as_array().is_some_and(Vec::is_empty) && store == StoreFile::Gestures {
        return Ok(T::default());
    }
    if value.as_array().is_some_and(Vec::is_empty)
        && matches!(
            store,
            StoreFile::Actions
                | StoreFile::Folders
                | StoreFile::Pins
                | StoreFile::Favorites
                | StoreFile::Usage
        )
    {
        return Ok(T::default());
    }
    serde_json::from_value(value.clone()).map_err(|_| {
        failure(
            RepairFailureKind::StoreMalformed,
            "A related saved-target store is malformed; no repair was written.",
        )
    })
}

fn array_mut(
    values: &mut HashMap<StoreFile, Value>,
    store: StoreFile,
) -> Result<&mut Vec<Value>, RepairFailure> {
    values
        .get_mut(&store)
        .and_then(Value::as_array_mut)
        .ok_or_else(owner_unavailable)
}

fn set_string_field(
    value: Option<&mut Value>,
    field: &str,
    replacement: &str,
) -> Result<(), RepairFailure> {
    let object = value
        .and_then(Value::as_object_mut)
        .ok_or_else(owner_unavailable)?;
    let field = object.get_mut(field).ok_or_else(owner_unavailable)?;
    *field = Value::String(replacement.to_owned());
    Ok(())
}

#[derive(Clone)]
struct FileChange {
    store: StoreFile,
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}

struct PreparedCommit {
    operation_id: u64,
    folders: Vec<FolderEntry>,
    favorites: Option<Vec<FavEntry>>,
    custom_actions: Option<Vec<Action>>,
}

fn persist_transaction<R, W>(
    root: &Path,
    changes: &[FileChange],
    reader: R,
    writer: W,
    live_usage: Option<&LiveUsageSnapshot>,
    custom_actions: Option<Vec<Action>>,
) -> Result<PreparedCommit, RepairFailure>
where
    R: Fn(&Path) -> Result<Option<Vec<u8>>, RepairFailure>,
    W: Fn(&Path, &[u8]) -> Result<(), ()>,
{
    let operation_id = NEXT_REPAIR_ID.fetch_add(1, Ordering::Relaxed);
    persist_transaction_inner(
        root,
        changes,
        reader,
        writer,
        live_usage,
        operation_id,
        None,
        custom_actions,
    )
}

fn persist_transaction_inner<R, W>(
    root: &Path,
    changes: &[FileChange],
    reader: R,
    writer: W,
    live_usage: Option<&LiveUsageSnapshot>,
    operation_id: u64,
    usage_operation: Option<&UsageOperation>,
    custom_actions: Option<Vec<Action>>,
) -> Result<PreparedCommit, RepairFailure>
where
    R: Fn(&Path) -> Result<Option<Vec<u8>>, RepairFailure>,
    W: Fn(&Path, &[u8]) -> Result<(), ()>,
{
    let before_folders = changes
        .iter()
        .find(|change| change.store == StoreFile::Folders)
        .and_then(|change| serde_json::from_slice::<Vec<FolderEntry>>(&change.after).ok());
    let after_favorites = changes
        .iter()
        .find(|change| change.store == StoreFile::Favorites)
        .and_then(|change| serde_json::from_slice::<Vec<FavEntry>>(&change.after).ok());
    if root.join(JOURNAL_FILE).exists() {
        return Err(recovery_conflict());
    }
    let mut journal = RepairJournal {
        version: JOURNAL_VERSION,
        operation_id,
        phase: JournalPhase::Prepared,
        files: changes
            .iter()
            .map(JournalFile::from_change)
            .collect::<Result<Vec<_>, _>>()?,
    };
    if custom_actions.is_some()
        && !changes
            .iter()
            .any(|change| change.store == StoreFile::Actions)
    {
        return Err(owner_unavailable());
    }
    if changes.is_empty() {
        if let Some(usage_operation) = usage_operation {
            usage_operation.mark_committed();
        }
        return Ok(PreparedCommit {
            operation_id,
            folders: before_folders.unwrap_or_default(),
            favorites: after_favorites,
            custom_actions,
        });
    }
    let prepared_bytes = match create_prepared_journal(root, &journal) {
        Ok(bytes) => bytes,
        Err(error) => {
            if error.kind == RepairFailureKind::RecoveryConflict
                && let Some(usage_operation) = usage_operation
            {
                usage_operation.mark_recovery_conflict();
            }
            return Err(error);
        }
    };
    for change in changes {
        let current = match reader(&change.path) {
            Ok(current) => current,
            Err(error) => {
                return rollback_after_failure(
                    root,
                    &journal.files,
                    &prepared_bytes,
                    &reader,
                    &writer,
                    error,
                    usage_operation,
                );
            }
        };
        if current != change.before {
            return rollback_after_failure(
                root,
                &journal.files,
                &prepared_bytes,
                &reader,
                &writer,
                stale_store_during_write(),
                usage_operation,
            );
        }
        if writer(&change.path, &change.after).is_err() {
            return rollback_after_failure(
                root,
                &journal.files,
                &prepared_bytes,
                &reader,
                &writer,
                failure(
                    RepairFailureKind::PersistFailed,
                    "A target repair could not be saved; original records were restored.",
                ),
                usage_operation,
            );
        }
    }
    journal.phase = JournalPhase::Committed;
    let usage_barrier = live_usage.map(|snapshot| {
        snapshot
            .mutation_barrier
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    });
    if live_usage.is_some_and(|snapshot| {
        snapshot.generation.load(Ordering::Acquire) != snapshot.observed_generation
    }) {
        return rollback_after_failure(
            root,
            &journal.files,
            &prepared_bytes,
            &reader,
            &writer,
            failure(
                RepairFailureKind::DestinationCollision,
                "Usage changed while the repair was being prepared; refresh and retry.",
            ),
            usage_operation,
        );
    }
    if let Err(marker_failure) = write_committed_journal(root, &journal, &prepared_bytes) {
        let rollback = rollback_changes(root, &journal.files, &reader, &writer);
        if rollback.is_err() {
            if let Some(usage_operation) = usage_operation {
                usage_operation.mark_recovery_conflict();
            }
            drop(usage_barrier);
            return Err(recovery_conflict());
        }
        let failure = match marker_failure {
            MarkerFailure::OwnedPrepared => {
                if remove_journal_if_bytes(root, &prepared_bytes).is_err() {
                    if let Some(usage_operation) = usage_operation {
                        usage_operation.mark_recovery_conflict();
                    }
                    drop(usage_barrier);
                    return Err(recovery_conflict());
                }
                failure(
                    RepairFailureKind::PersistFailed,
                    "The target repair could not record its committed outcome; original files were restored.",
                )
            }
            MarkerFailure::Missing => failure(
                RepairFailureKind::PersistFailed,
                "The target repair journal disappeared; original files were restored.",
            ),
            MarkerFailure::Foreign => {
                if let Some(usage_operation) = usage_operation {
                    usage_operation.mark_recovery_conflict();
                }
                drop(usage_barrier);
                return Err(recovery_conflict());
            }
        };
        if let Some(usage_operation) = usage_operation {
            usage_operation.mark_restored();
        }
        drop(usage_barrier);
        return Err(failure);
    }
    if let Some(usage_operation) = usage_operation {
        usage_operation.mark_committed();
    }
    drop(usage_barrier);
    // A committed marker makes a leftover journal safe: startup recovery only
    // removes it and never reverses a completed repair.
    let committed_bytes = encode_journal(&journal)?;
    if remove_journal_if_bytes(root, &committed_bytes).is_err() {
        let committed_journal_remains =
            fs::read(root.join(JOURNAL_FILE)).is_ok_and(|current| current == committed_bytes);
        if !committed_journal_remains {
            return Err(recovery_conflict());
        }
    }
    Ok(PreparedCommit {
        operation_id,
        folders: before_folders.unwrap_or_default(),
        favorites: after_favorites,
        custom_actions,
    })
}

fn write_store(path: &Path, bytes: &[u8]) -> Result<(), ()> {
    save_atomic(path, bytes).map_err(|_| ())
}

fn rollback_after_failure<R, W>(
    root: &Path,
    journal_files: &[JournalFile],
    expected_prepared_bytes: &[u8],
    reader: &R,
    writer: &W,
    original_error: RepairFailure,
    usage_operation: Option<&UsageOperation>,
) -> Result<PreparedCommit, RepairFailure>
where
    R: Fn(&Path) -> Result<Option<Vec<u8>>, RepairFailure>,
    W: Fn(&Path, &[u8]) -> Result<(), ()>,
{
    if rollback_changes(root, journal_files, reader, writer).is_err() {
        if let Some(usage_operation) = usage_operation {
            usage_operation.mark_recovery_conflict();
        }
        return Err(recovery_conflict());
    }
    if remove_journal_if_bytes(root, expected_prepared_bytes).is_err() {
        if let Some(usage_operation) = usage_operation {
            usage_operation.mark_recovery_conflict();
        }
        return Err(recovery_conflict());
    }
    if let Some(usage_operation) = usage_operation {
        usage_operation.mark_restored();
    }
    Err(original_error)
}

fn rollback_changes<R, W>(
    root: &Path,
    journal_files: &[JournalFile],
    reader: &R,
    writer: &W,
) -> Result<(), ()>
where
    R: Fn(&Path) -> Result<Option<Vec<u8>>, RepairFailure>,
    W: Fn(&Path, &[u8]) -> Result<(), ()>,
{
    for file in journal_files.iter().rev() {
        let path = root.join(file.store.file_name());
        let current = reader(&path).map_err(|_| ())?;
        let after = Some(file.after.as_bytes().to_vec());
        let before = file.before.as_ref().map(|bytes| bytes.as_bytes().to_vec());
        if current == before {
            continue;
        }
        if current != after {
            return Err(());
        }
        if let Some(before) = before {
            writer(&path, &before)?;
        } else {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(()),
            }
        }
        if reader(&path).map_err(|_| ())?
            != file.before.as_ref().map(|value| value.as_bytes().to_vec())
        {
            return Err(());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JournalPhase {
    Prepared,
    Committed,
}

#[derive(Serialize, Deserialize)]
struct RepairJournal {
    version: u32,
    operation_id: u64,
    phase: JournalPhase,
    files: Vec<JournalFile>,
}

impl fmt::Debug for RepairJournal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepairJournal")
            .field("version", &self.version)
            .field("phase", &self.phase)
            .field("file_count", &self.files.len())
            .field("contents", &"[redacted]")
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct JournalFile {
    store: StoreFile,
    before: Option<String>,
    after: String,
}

impl JournalFile {
    fn from_change(change: &FileChange) -> Result<Self, RepairFailure> {
        let before = change
            .before
            .as_deref()
            .map(std::str::from_utf8)
            .transpose()
            .map_err(|_| {
                failure(
                    RepairFailureKind::StoreMalformed,
                    "A related store is not valid UTF-8.",
                )
            })?
            .map(str::to_owned);
        let after = String::from_utf8(change.after.clone()).map_err(|_| {
            failure(
                RepairFailureKind::StoreMalformed,
                "A related store is not valid UTF-8.",
            )
        })?;
        Ok(Self {
            store: change.store,
            before,
            after,
        })
    }
}

impl fmt::Debug for JournalFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JournalFile")
            .field("store", &self.store)
            .field("before", &self.before.is_some())
            .field("after", &"[redacted]")
            .finish()
    }
}

fn encode_journal(journal: &RepairJournal) -> Result<Vec<u8>, RepairFailure> {
    let bytes = serde_json::to_vec(journal).map_err(|_| recovery_conflict())?;
    if bytes.len() > MAX_REPAIR_JOURNAL_BYTES {
        return Err(recovery_conflict());
    }
    Ok(bytes)
}

/// Create a prepared journal without replacing a prior unresolved record.
/// The hard link is the atomic no-clobber publication step.
fn create_prepared_journal(root: &Path, journal: &RepairJournal) -> Result<Vec<u8>, RepairFailure> {
    let bytes = encode_journal(journal)?;
    fs::create_dir_all(root).map_err(|_| recovery_conflict())?;
    let id = NEXT_REPAIR_ID.fetch_add(1, Ordering::Relaxed);
    let staging = root.join(format!(".{JOURNAL_FILE}.{}.{}.tmp", std::process::id(), id));
    save_atomic(&staging, &bytes).map_err(|_| recovery_conflict())?;
    let destination = root.join(JOURNAL_FILE);
    let linked = fs::hard_link(&staging, &destination).is_ok();
    let _ = fs::remove_file(&staging);
    if !linked {
        return Err(recovery_conflict());
    }
    Ok(bytes)
}

fn write_committed_journal(
    root: &Path,
    journal: &RepairJournal,
    expected_prepared_bytes: &[u8],
) -> Result<(), MarkerFailure> {
    let destination = root.join(JOURNAL_FILE);
    let current = match fs::read(&destination) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(MarkerFailure::Missing);
        }
        Err(_) => return Err(MarkerFailure::Foreign),
    };
    if current != expected_prepared_bytes {
        if decode_committed_journal(&current, journal.operation_id) {
            return Ok(());
        }
        return Err(MarkerFailure::Foreign);
    }
    let bytes = encode_journal(journal).map_err(|_| MarkerFailure::OwnedPrepared)?;
    if save_atomic(&destination, &bytes).is_ok() {
        return Ok(());
    }
    match fs::read(&destination) {
        Ok(current) if current == bytes => Ok(()),
        Ok(current) if current == expected_prepared_bytes => Err(MarkerFailure::OwnedPrepared),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(MarkerFailure::Missing),
        _ => Err(MarkerFailure::Foreign),
    }
}

#[derive(Clone, Copy)]
enum MarkerFailure {
    OwnedPrepared,
    Missing,
    Foreign,
}

fn decode_committed_journal(bytes: &[u8], operation_id: u64) -> bool {
    serde_json::from_slice::<RepairJournal>(bytes).is_ok_and(|journal| {
        journal.operation_id == operation_id && journal.phase == JournalPhase::Committed
    })
}

fn remove_journal_if_bytes(root: &Path, expected: &[u8]) -> std::io::Result<()> {
    let path = root.join(JOURNAL_FILE);
    let current = fs::read(&path)?;
    if current != expected {
        return Err(std::io::Error::other("target repair journal changed"));
    }
    fs::remove_file(path)
}

/// Recover a prepared target repair before persisted owners are admitted.
pub fn recover_startup(root: impl AsRef<Path>) -> Result<(), RepairFailure> {
    let root = root.as_ref();
    let _transaction = REPAIR_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let paths = repair_fence_paths(root);
    match fs::metadata(root.join(JOURNAL_FILE)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return if crate::common::repair_watch::any_blocked(paths) {
                Err(recovery_conflict())
            } else {
                Ok(())
            };
        }
        Err(_) => {
            crate::common::repair_watch::block_paths(paths);
            return Err(recovery_conflict());
        }
        Ok(_) => {}
    }
    let result = recover_startup_locked(root);
    match result {
        Ok(()) => {
            crate::common::repair_watch::resolve_recovered_paths(paths);
            Ok(())
        }
        Err(error) => {
            crate::common::repair_watch::block_paths(paths);
            Err(error)
        }
    }
}

fn recover_startup_locked(root: &Path) -> Result<(), RepairFailure> {
    let journal_path = root.join(JOURNAL_FILE);
    let bytes = match fs::read(&journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(recovery_conflict()),
    };
    if bytes.len() > MAX_REPAIR_JOURNAL_BYTES {
        return Err(recovery_conflict());
    }
    let journal: RepairJournal = serde_json::from_slice(&bytes).map_err(|_| recovery_conflict())?;
    if journal.version != JOURNAL_VERSION || journal.files.len() > 6 {
        return Err(recovery_conflict());
    }
    let mut seen = std::collections::HashSet::new();
    if journal.files.iter().any(|file| {
        !seen.insert(file.store)
            || file.after.len() > MAX_REPAIR_STORE_BYTES
            || file
                .before
                .as_ref()
                .is_some_and(|before| before.len() > MAX_REPAIR_STORE_BYTES)
    }) {
        return Err(recovery_conflict());
    }
    if journal.phase == JournalPhase::Committed {
        remove_journal_if_bytes(root, &bytes).map_err(|_| recovery_conflict())?;
        return Ok(());
    }
    for file in journal.files.iter().rev() {
        let path = root.join(file.store.file_name());
        let current = read_store(&path).map_err(|_| recovery_conflict())?;
        let after = Some(file.after.as_bytes().to_vec());
        let before = file.before.as_ref().map(|bytes| bytes.as_bytes().to_vec());
        if current == before {
            continue;
        }
        if current != after {
            return Err(recovery_conflict());
        }
        if let Some(before) = before {
            save_atomic(&path, &before).map_err(|_| recovery_conflict())?;
        } else {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(recovery_conflict()),
            }
        }
        let restored = read_store(&path).map_err(|_| recovery_conflict())?;
        if restored != file.before.as_ref().map(|value| value.as_bytes().to_vec()) {
            return Err(recovery_conflict());
        }
    }
    remove_journal_if_bytes(root, &bytes).map_err(|_| recovery_conflict())
}

fn failure(kind: RepairFailureKind, message: &'static str) -> RepairFailure {
    RepairFailure { kind, message }
}

fn stale_selection() -> RepairFailure {
    failure(
        RepairFailureKind::StaleSelection,
        "The selected saved target changed; refresh the list before repairing.",
    )
}

fn stale_store_during_write() -> RepairFailure {
    failure(
        RepairFailureKind::StoreUnavailable,
        "A related saved store changed during repair; the transaction was safely stopped.",
    )
}

fn owner_unavailable() -> RepairFailure {
    failure(
        RepairFailureKind::StoreUnavailable,
        "A saved-target owner could not be verified; no repair was written.",
    )
}

fn recovery_conflict() -> RepairFailure {
    failure(
        RepairFailureKind::RecoveryConflict,
        "Target repair recovery needs attention; saved records were not overwritten.",
    )
}

pub struct RepairCompletion {
    pub request: RepairRequest,
    pub result: Result<RepairCommit, RepairFailure>,
}

impl fmt::Debug for RepairCompletion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepairCompletion")
            .field("request", &self.request)
            .field(
                "result",
                &self
                    .result
                    .as_ref()
                    .map(|_| "committed")
                    .map_err(|error| error.kind),
            )
            .finish()
    }
}

enum RepairCommand {
    Repair(RepairRequest),
}

#[derive(Default)]
struct RepairWorkState {
    admitted: usize,
    pending_completions: usize,
    closed: bool,
}

enum RepairAdmissionError {
    Closed,
    Full,
}

#[derive(Default)]
struct RepairWorkLifecycle {
    state: Mutex<RepairWorkState>,
    idle: Condvar,
    #[cfg(test)]
    wait_observer: Mutex<Option<mpsc::Sender<()>>>,
    #[cfg(test)]
    start_observer: Mutex<Option<mpsc::Sender<()>>>,
}

impl RepairWorkLifecycle {
    fn admit(&self) -> Result<(), RepairAdmissionError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Err(RepairAdmissionError::Closed);
        }
        if state.pending_completions >= REPAIR_QUEUE_CAPACITY {
            return Err(RepairAdmissionError::Full);
        }
        state.admitted += 1;
        state.pending_completions += 1;
        Ok(())
    }

    fn cancel_admission(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        debug_assert!(state.admitted > 0, "repair work admission is balanced");
        debug_assert!(
            state.pending_completions > 0,
            "repair completion reservations are balanced"
        );
        state.admitted = state.admitted.saturating_sub(1);
        state.pending_completions = state.pending_completions.saturating_sub(1);
        if state.admitted == 0 {
            self.idle.notify_all();
        }
    }

    fn finish_one(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        debug_assert!(state.admitted > 0, "repair work admission is balanced");
        state.admitted = state.admitted.saturating_sub(1);
        if state.admitted == 0 {
            self.idle.notify_all();
        }
    }

    fn close_admission(&self) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .closed = true;
    }

    fn acknowledge_completions(&self, count: usize) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        debug_assert!(state.pending_completions >= count);
        state.pending_completions = state.pending_completions.saturating_sub(count);
    }

    fn wait_until_idle(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.admitted > 0 {
            #[cfg(test)]
            if let Some(sender) = self
                .wait_observer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                let _ = sender.send(());
            }
        }
        while state.admitted > 0 {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    #[cfg(test)]
    fn observe_next_wait(&self, sender: mpsc::Sender<()>) {
        *self
            .wait_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
    }

    #[cfg(test)]
    fn observe_next_start(&self, sender: mpsc::Sender<()>) {
        *self
            .start_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
    }

    #[cfg(test)]
    fn notify_started(&self) {
        if let Some(sender) = self
            .start_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = sender.send(());
        }
    }
}

struct RepairWorkGuard(Arc<RepairWorkLifecycle>);

impl Drop for RepairWorkGuard {
    fn drop(&mut self) {
        self.0.finish_one();
    }
}

/// App-owned asynchronous client. Its worker is joined by the bounded global
/// reaper during shutdown; no GUI frame waits for a repair or thread exit.
pub struct TargetRepairClient {
    request_tx: Option<SyncSender<RepairCommand>>,
    completion_rx: Receiver<RepairCompletion>,
    worker: Option<JoinHandle<()>>,
    reaper_permit: Option<crate::thread_reaper::ReapPermit>,
    usage_handoff: UsageHandoff,
    mutation_barrier: Arc<Mutex<()>>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    lifecycle: Arc<RepairWorkLifecycle>,
}

impl TargetRepairClient {
    pub fn new(
        root: impl Into<PathBuf>,
        radial: Arc<RadialStore>,
        hotfolder: HotFolderRulesStore,
        usage_handoff: UsageHandoff,
        mutation_barrier: Arc<Mutex<()>>,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, RepairFailure> {
        let permit = crate::thread_reaper::reserve().map_err(|_| {
            failure(
                RepairFailureKind::WorkerUnavailable,
                "Target repair could not start because worker capacity is unavailable.",
            )
        })?;
        let coordinator = TargetRepairCoordinator::new(root, radial, hotfolder);
        let (request_tx, request_rx) = mpsc::sync_channel(REPAIR_QUEUE_CAPACITY);
        let (completion_tx, completion_rx) = mpsc::sync_channel(REPAIR_QUEUE_CAPACITY);
        let lifecycle = Arc::new(RepairWorkLifecycle::default());
        let lifecycle_for_worker = Arc::clone(&lifecycle);
        let completion_notifier = permit.completion_notifier();
        let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(repaint);
        let repaint_for_worker = Arc::clone(&repaint);
        let worker = thread::Builder::new()
            .name("target-repair-owner".to_owned())
            .spawn(move || {
                // Declared first so it is dropped last, after the receiver and
                // coordinator are gone, and wakes the bounded join reaper.
                let _completion_notifier = completion_notifier;
                while let Ok(RepairCommand::Repair(request)) = request_rx.recv() {
                    let work_guard = RepairWorkGuard(Arc::clone(&lifecycle_for_worker));
                    #[cfg(test)]
                    lifecycle_for_worker.notify_started();
                    let result = coordinator.repair(request.clone());
                    // The repair's path fence is resolved before repair returns.
                    // Exit may proceed once this guard drops; it does not depend
                    // on the GUI draining the bounded completion channel.
                    drop(work_guard);
                    match completion_tx.try_send(RepairCompletion { request, result }) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => {
                            // Admission reserves one bounded completion slot
                            // for each accepted request, so this is an internal
                            // invariant violation rather than backpressure.
                            tracing::error!("target repair completion capacity invariant failed");
                        }
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                    (repaint_for_worker)();
                }
            })
            .map_err(|_| {
                failure(
                    RepairFailureKind::WorkerUnavailable,
                    "Target repair could not start its owner worker.",
                )
            })?;
        Ok(Self {
            request_tx: Some(request_tx),
            completion_rx,
            worker: Some(worker),
            reaper_permit: Some(permit),
            usage_handoff,
            mutation_barrier,
            repaint,
            lifecycle,
        })
    }

    pub fn submit(&self, request: RepairRequest) -> Result<(), RepairFailure> {
        if self.usage_handoff.has_pending() {
            return Err(failure(
                RepairFailureKind::QueueFull,
                "A target repair is still reconciling usage; wait before starting another.",
            ));
        }
        let sender = self.request_tx.as_ref().ok_or_else(|| {
            failure(
                RepairFailureKind::WorkerUnavailable,
                "Target repair is shutting down.",
            )
        })?;
        match self.lifecycle.admit() {
            Ok(()) => {}
            Err(RepairAdmissionError::Closed) => {
                return Err(failure(
                    RepairFailureKind::WorkerUnavailable,
                    "Target repair is shutting down.",
                ));
            }
            Err(RepairAdmissionError::Full) => {
                return Err(failure(
                    RepairFailureKind::QueueFull,
                    "Target repair results are waiting to be received; wait before starting another.",
                ));
            }
        }
        match sender.try_send(RepairCommand::Repair(request)) {
            Ok(()) => {
                (self.repaint)();
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                self.lifecycle.cancel_admission();
                Err(failure(
                    RepairFailureKind::QueueFull,
                    "Target repair is busy; wait for the current operation to finish.",
                ))
            }
            Err(TrySendError::Disconnected(_)) => {
                self.lifecycle.cancel_admission();
                Err(failure(
                    RepairFailureKind::WorkerUnavailable,
                    "Target repair is unavailable.",
                ))
            }
        }
    }

    pub fn poll_completions(&self) -> Vec<RepairCompletion> {
        let mut completions = Vec::new();
        while let Ok(completion) = self.completion_rx.try_recv() {
            completions.push(completion);
        }
        self.lifecycle.acknowledge_completions(completions.len());
        completions
    }

    pub(crate) fn apply_usage_handoff(
        &self,
        usage_map: &mut HashMap<String, u32>,
        deferred_usage: &Mutex<HashMap<String, u32>>,
    ) -> UsageHandoffApply {
        self.usage_handoff.apply_ready(usage_map, deferred_usage)
    }

    pub fn save_usage_on_exit(
        &mut self,
        path: &str,
        usage_map: &mut HashMap<String, u32>,
        deferred_usage: &Mutex<HashMap<String, u32>>,
    ) -> anyhow::Result<()> {
        self.stop_admission_and_wait_for_repairs();
        if self.usage_handoff.pending_blocks_path(Path::new(path)) {
            anyhow::bail!("usage save is blocked until target-repair recovery resolves");
        }
        let _path_permit = crate::common::repair_watch::require_path_read(Path::new(path))?;
        let _transaction = usage::transaction_guard();
        let _mutation_barrier = self
            .mutation_barrier
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let applied = self.usage_handoff.apply_ready(usage_map, deferred_usage);
        if applied.conflict || self.usage_handoff.has_pending() {
            anyhow::bail!("usage save is blocked until target-repair usage is reconciled");
        }
        usage::save_usage_locked(path, usage_map)
    }

    pub fn shutdown(&mut self) {
        self.stop_admission_and_wait_for_repairs();
        if let (Some(worker), Some(permit)) = (self.worker.take(), self.reaper_permit.take())
            && let Err(error) = permit.reap(worker)
        {
            tracing::error!("target repair join supervision failed: {error}");
        }
    }

    fn stop_admission_and_wait_for_repairs(&mut self) {
        self.lifecycle.close_admission();
        // Closing the sender lets the worker drain already-admitted requests
        // and then observe disconnection. The lifecycle count includes both
        // queued and active work, so no queued repair can start after the wait.
        self.request_tx.take();
        self.lifecycle.wait_until_idle();
    }
}

impl Drop for TargetRepairClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ACTION_REPAIR_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn action(path: &str, args: Option<&str>) -> Action {
        Action {
            label: "Saved label".into(),
            desc: "Saved description".into(),
            action: path.into(),
            args: args.map(str::to_owned),
        }
    }

    fn write_json(path: &Path, value: &impl Serialize) {
        fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }

    fn write_documents(
        root: &Path,
        actions: &[Action],
        folders: &[FolderEntry],
        pins: &[crate::history::HistoryPin],
        favorites: &[FavEntry],
        usage_entries: &[UsageEntry],
    ) {
        fs::create_dir_all(root).unwrap();
        write_json(&root.join(StoreFile::Actions.file_name()), &actions);
        write_json(&root.join(StoreFile::Folders.file_name()), &folders);
        write_json(&root.join(StoreFile::Pins.file_name()), &pins);
        write_json(&root.join(StoreFile::Favorites.file_name()), &favorites);
        write_json(&root.join(StoreFile::Usage.file_name()), &usage_entries);
    }

    #[test]
    fn target_repair_plan_preserves_metadata_and_only_rewrites_exact_associations() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let original = action("C:\\missing\\tool.exe", Some("--profile=blue"));
        let pin_exact = crate::history::HistoryPin {
            action_id: original.action.clone(),
            label: "Pinned label".into(),
            desc: "Pinned description".into(),
            args: original.args.clone(),
            query: "private query remains untouched".into(),
            timestamp: 17,
        };
        let pin_other_args = crate::history::HistoryPin {
            args: Some("--profile=red".into()),
            ..pin_exact.clone()
        };
        let fav_exact = FavEntry {
            label: "Favorite label".into(),
            action: original.action.clone(),
            args: original.args.clone(),
        };
        let fav_other_args = FavEntry {
            args: Some("--profile=red".into()),
            ..fav_exact.clone()
        };
        write_documents(
            root,
            std::slice::from_ref(&original),
            &[],
            &[pin_exact, pin_other_args],
            &[fav_exact, fav_other_args],
            &[UsageEntry {
                action: original.action.clone(),
                count: 4,
            }],
        );
        let mut documents = StoreDocuments::load(&store_paths(root)).unwrap();
        let (updated, usage_rekey) = documents
            .repair_action(&original, "D:\\tools\\replacement.exe", None)
            .unwrap();

        assert_eq!(updated.label, original.label);
        assert_eq!(updated.desc, original.desc);
        assert_eq!(updated.args, original.args);
        assert_eq!(updated.action, "D:\\tools\\replacement.exe");
        assert_eq!(
            usage_rekey,
            Some((original.action.clone(), updated.action.clone()))
        );
        assert_eq!(documents.pins[0].action_id, updated.action);
        assert_eq!(documents.pins[1].action_id, original.action);
        assert_eq!(documents.pins[0].query, "private query remains untouched");
        assert_eq!(documents.favorites[0].action, updated.action);
        assert_eq!(documents.favorites[1].action, original.action);
        assert_eq!(documents.usage[0].action, updated.action);
    }

    #[test]
    fn target_repair_plan_uses_live_usage_and_refuses_shared_or_cached_destinations() {
        let directory = tempfile::tempdir().unwrap();
        let shared_root = directory.path().join("shared");
        let original = action("C:\\missing\\tool.exe", None);
        let folder = FolderEntry {
            label: "same source".into(),
            path: original.action.clone(),
            alias: Some("keep-alias".into()),
        };
        write_documents(
            &shared_root,
            std::slice::from_ref(&original),
            &[folder],
            &[],
            &[],
            &[UsageEntry {
                action: original.action.clone(),
                count: 2,
            }],
        );
        let mut documents = StoreDocuments::load(&store_paths(&shared_root)).unwrap();
        let live = HashMap::from([(original.action.clone(), 12)]);
        let error = documents
            .repair_action(&original, "D:\\tools\\new.exe", Some(&live))
            .unwrap_err();
        assert_eq!(error.kind, RepairFailureKind::SharedUsageKey);
        assert!(documents.dirty.is_empty());

        let live_root = directory.path().join("live");
        write_documents(
            &live_root,
            std::slice::from_ref(&original),
            &[],
            &[],
            &[],
            &[],
        );
        let mut documents = StoreDocuments::load(&store_paths(&live_root)).unwrap();
        let (updated, rekey) = documents
            .repair_action(&original, "D:\\tools\\new.exe", Some(&live))
            .unwrap();
        assert_eq!(updated.action, "D:\\tools\\new.exe");
        assert_eq!(
            rekey,
            Some((original.action.clone(), updated.action.clone()))
        );
        assert_eq!(documents.usage[0].count, 12);

        // A destination already used by the live application map is not
        // visible in usage.json yet, but is still a collision.
        let mut documents = StoreDocuments::load(&store_paths(&live_root)).unwrap();
        let live_with_destination = HashMap::from([
            (original.action.clone(), 12),
            ("D:\\tools\\new.exe".into(), 1),
        ]);
        let error = documents
            .repair_action(
                &original,
                "D:\\tools\\new.exe",
                Some(&live_with_destination),
            )
            .unwrap_err();
        assert_eq!(error.kind, RepairFailureKind::DestinationCollision);
        assert!(documents.dirty.is_empty());
    }

    #[test]
    fn target_repair_plan_refuses_unattributable_path_only_associations_without_usage() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let original = action("C:\\missing\\shared-target", None);
        let folder = FolderEntry {
            label: "same filesystem target".into(),
            path: original.action.clone(),
            alias: Some("folder alias is preserved".into()),
        };
        let pin = crate::history::HistoryPin {
            action_id: original.action.clone(),
            label: "path-only pin".into(),
            desc: "the pin cannot identify which owner it references".into(),
            args: None,
            query: "private query stays untouched".into(),
            timestamp: 9,
        };
        let favorite = FavEntry {
            label: "path-only favorite".into(),
            action: original.action.clone(),
            args: None,
        };
        write_documents(
            root,
            std::slice::from_ref(&original),
            &[folder],
            &[pin],
            &[favorite],
            &[],
        );

        let mut documents = StoreDocuments::load(&store_paths(root)).unwrap();
        let error = documents
            .repair_action(&original, "D:\\repaired\\target", None)
            .unwrap_err();

        assert_eq!(error.kind, RepairFailureKind::AmbiguousSelection);
        assert!(documents.dirty.is_empty());
        assert_eq!(documents.pins[0].query, "private query stays untouched");
        assert_eq!(documents.favorites[0].action, original.action);
    }

    #[test]
    fn target_repair_plan_accepts_directory_replacement_for_custom_action() {
        let _serial = ACTION_REPAIR_TEST_LOCK.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let original = action("C:\\missing\\custom-directory", None);
        write_documents(root, std::slice::from_ref(&original), &[], &[], &[], &[]);
        let replacement = root.join("replacement-directory");
        fs::create_dir(&replacement).unwrap();

        let app_root = AppDataRoot::from_path(root.to_path_buf());
        let hotfolder = HotFolderRulesStore::new(&app_root);
        let radial = Arc::new(RadialStore::new(&app_root).unwrap());
        let coordinator = TargetRepairCoordinator::new(root, radial, hotfolder);
        let result = coordinator
            .repair(RepairRequest::new(
                SelectedTarget::action_with_kind(original, 0, ActionTargetKind::FileOrFolder),
                replacement.clone(),
            ))
            .unwrap();

        assert_eq!(
            result.updated_action.unwrap().action,
            replacement.to_string_lossy().into_owned()
        );
        let persisted: Vec<Action> =
            serde_json::from_slice(&fs::read(root.join(StoreFile::Actions.file_name())).unwrap())
                .unwrap();
        assert_eq!(
            persisted[0].action,
            replacement.to_string_lossy().into_owned()
        );
    }

    #[test]
    fn target_repair_plan_refuses_exact_radial_and_hotfolder_references_without_writes() {
        fn snapshot_stores(root: &Path) -> HashMap<StoreFile, Option<Vec<u8>>> {
            store_paths(root)
                .into_iter()
                .map(|(store, path)| {
                    let bytes = match fs::read(path) {
                        Ok(bytes) => Some(bytes),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                        Err(error) => panic!("failed to snapshot a test store: {error}"),
                    };
                    (store, bytes)
                })
                .collect()
        }

        let _serial = ACTION_REPAIR_TEST_LOCK.lock().unwrap();
        let original = action("C:\\missing\\referenced-tool.exe", Some("--profile=blue"));

        let radial_directory = tempfile::tempdir().unwrap();
        let radial_root = radial_directory.path();
        write_documents(
            radial_root,
            std::slice::from_ref(&original),
            &[],
            &[],
            &[],
            &[],
        );
        let replacement = radial_root.join("replacement.exe");
        fs::write(&replacement, b"replacement").unwrap();
        let mut radial_document = crate::radial::model::RadialDocument::starter();
        let cell = radial_document
            .menus
            .iter_mut()
            .find(|menu| menu.id.as_str() == "starter-screen-tools")
            .and_then(|menu| menu.rings.first_mut())
            .and_then(|ring| ring.cells.first_mut())
            .expect("starter screen-tools cell exists");
        cell.content = crate::radial::model::CellContent::Action {
            binding: crate::radial::model::ActionBinding::ExactCommand {
                command: original.action.clone(),
                args: original.args.clone(),
            },
        };
        let radial_path = radial_root.join("radial.json");
        let radial = Arc::new(RadialStore::at_path(&radial_path, radial_document.clone()).unwrap());
        let app_root = AppDataRoot::from_path(radial_root.to_path_buf());
        let hotfolder = HotFolderRulesStore::new(&app_root);
        let coordinator = TargetRepairCoordinator::new(radial_root, radial, hotfolder);
        let bytes_before = snapshot_stores(radial_root);

        let error = coordinator
            .repair(RepairRequest::new(
                SelectedTarget::action(original.clone(), 0),
                replacement,
            ))
            .unwrap_err();

        assert_eq!(error.kind, RepairFailureKind::ActiveRadialReference);
        assert_eq!(snapshot_stores(radial_root), bytes_before);
        assert!(!radial_root.join(JOURNAL_FILE).exists());

        let hotfolder_directory = tempfile::tempdir().unwrap();
        let hotfolder_root = hotfolder_directory.path();
        write_documents(
            hotfolder_root,
            std::slice::from_ref(&original),
            &[],
            &[],
            &[],
            &[],
        );
        let app_root = AppDataRoot::from_path(hotfolder_root.to_path_buf());
        let hotfolder = HotFolderRulesStore::new(&app_root);
        assert_eq!(
            hotfolder.load().status,
            crate::hotfolder::HotFolderLoadStatus::Missing
        );
        let target = crate::universal_actions::PersistableActionTargetRef::CustomAction {
            action: original.clone(),
        };
        let reference = crate::universal_actions::PersistedUniversalActionRef {
            target: Some(target),
            action_id: crate::universal_actions::action_ids::RESULT_EXECUTE,
        };
        let fingerprint =
            crate::hotfolder::ConfigurationFingerprint::of_serializable(&reference).unwrap();
        let rule = crate::hotfolder::HotFolderRule::new(
            "disabled but authorization-bound",
            hotfolder_root.join("watched"),
            crate::hotfolder::RuleAction::RegisteredLauncherAction {
                reference,
                configuration_fingerprint: fingerprint,
            },
        );
        hotfolder.add_rule(0, rule).unwrap();
        let hotfolder_bytes_before = fs::read(hotfolder.path()).unwrap();
        let bytes_before = snapshot_stores(hotfolder_root);
        let replacement = hotfolder_root.join("replacement.exe");
        fs::write(&replacement, b"replacement").unwrap();
        let radial = Arc::new(RadialStore::new(&app_root).unwrap());
        let coordinator = TargetRepairCoordinator::new(hotfolder_root, radial, hotfolder.clone());

        let error = coordinator
            .repair(RepairRequest::new(
                SelectedTarget::action(original, 0),
                replacement,
            ))
            .unwrap_err();

        assert_eq!(error.kind, RepairFailureKind::HotFolderAuthorization);
        assert_eq!(snapshot_stores(hotfolder_root), bytes_before);
        assert_eq!(fs::read(hotfolder.path()).unwrap(), hotfolder_bytes_before);
        assert!(!hotfolder_root.join(JOURNAL_FILE).exists());
    }

    #[test]
    fn target_repair_plan_rejects_malformed_related_store_without_mutating_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let original = action("C:\\missing\\tool.exe", None);
        write_documents(root, std::slice::from_ref(&original), &[], &[], &[], &[]);
        let actions_path = root.join(StoreFile::Actions.file_name());
        let malformed_pins = b"{not a pins array";
        let pins_path = root.join(StoreFile::Pins.file_name());
        fs::write(&pins_path, malformed_pins).unwrap();
        let actions_before = fs::read(&actions_path).unwrap();

        let error = match StoreDocuments::load(&store_paths(root)) {
            Ok(_) => panic!("malformed related stores must refuse repair preflight"),
            Err(error) => error,
        };

        assert_eq!(error.kind, RepairFailureKind::StoreMalformed);
        assert_eq!(fs::read(&actions_path).unwrap(), actions_before);
        assert_eq!(fs::read(&pins_path).unwrap(), malformed_pins);
        assert!(!root.join(JOURNAL_FILE).exists());
    }

    fn test_change(store: StoreFile, root: &Path, before: &[u8], after: &[u8]) -> FileChange {
        let path = root.join(store.file_name());
        fs::write(&path, before).unwrap();
        FileChange {
            store,
            path,
            before: Some(before.to_vec()),
            after: after.to_vec(),
        }
    }

    fn disk_reader(path: &Path) -> Result<Option<Vec<u8>>, RepairFailure> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(failure(
                RepairFailureKind::StoreUnavailable,
                "store read failed",
            )),
        }
    }

    #[test]
    fn target_repair_transaction_restores_prior_files_after_later_write_failure() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let changes = vec![
            test_change(
                StoreFile::Actions,
                root,
                b"actions-before",
                b"actions-after",
            ),
            test_change(
                StoreFile::Folders,
                root,
                b"folders-before",
                b"folders-after",
            ),
        ];
        let writes = AtomicU64::new(0);
        let result = persist_transaction(
            root,
            &changes,
            disk_reader,
            |path, bytes| {
                let index = writes.fetch_add(1, Ordering::AcqRel);
                if index == 1 {
                    return Err(());
                }
                fs::write(path, bytes).map_err(|_| ())
            },
            None,
            None,
        );
        assert_eq!(result.err().unwrap().kind, RepairFailureKind::PersistFailed);
        assert_eq!(
            fs::read(changes[0].path.as_path()).unwrap(),
            b"actions-before"
        );
        assert_eq!(
            fs::read(changes[1].path.as_path()).unwrap(),
            b"folders-before"
        );
        assert!(!root.join(JOURNAL_FILE).exists());
    }

    #[test]
    fn target_repair_transaction_rolls_back_after_a_later_store_read_failure() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let changes = vec![
            test_change(
                StoreFile::Actions,
                root,
                b"actions-before",
                b"actions-after",
            ),
            test_change(
                StoreFile::Folders,
                root,
                b"folders-before",
                b"folders-after",
            ),
        ];
        let folder_reads = AtomicU64::new(0);
        let result = persist_transaction(
            root,
            &changes,
            |path| {
                if path == changes[1].path && folder_reads.fetch_add(1, Ordering::AcqRel) == 0 {
                    return Err(failure(
                        RepairFailureKind::StoreUnavailable,
                        "injected later store read failure",
                    ));
                }
                disk_reader(path)
            },
            write_store,
            None,
            None,
        );

        assert_eq!(
            result.err().unwrap().kind,
            RepairFailureKind::StoreUnavailable
        );
        assert_eq!(fs::read(&changes[0].path).unwrap(), b"actions-before");
        assert_eq!(fs::read(&changes[1].path).unwrap(), b"folders-before");
        assert!(!root.join(JOURNAL_FILE).exists());
    }

    #[test]
    fn target_repair_transaction_conflict_retains_journal_and_blocks_partial_reads() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let changes = vec![
            test_change(
                StoreFile::Actions,
                root,
                b"actions-before",
                b"actions-after",
            ),
            test_change(
                StoreFile::Folders,
                root,
                b"folders-before",
                b"folders-after",
            ),
        ];
        let writes = AtomicU64::new(0);
        let result = persist_transaction(
            root,
            &changes,
            disk_reader,
            |path, bytes| {
                let index = writes.fetch_add(1, Ordering::AcqRel);
                match index {
                    0 => fs::write(path, bytes).map_err(|_| ()),
                    1 => {
                        fs::write(&changes[0].path, b"external edit").map_err(|_| ())?;
                        Err(())
                    }
                    _ => fs::write(path, bytes).map_err(|_| ()),
                }
            },
            None,
            None,
        );
        assert_eq!(
            result.err().unwrap().kind,
            RepairFailureKind::RecoveryConflict
        );
        let journal_path = root.join(JOURNAL_FILE);
        let original_journal = fs::read(&journal_path).unwrap();
        assert_eq!(fs::read(&changes[0].path).unwrap(), b"external edit");

        let second_attempt = persist_transaction(
            root,
            &changes,
            disk_reader,
            |path, bytes| fs::write(path, bytes).map_err(|_| ()),
            None,
            None,
        );
        assert_eq!(
            second_attempt.err().unwrap().kind,
            RepairFailureKind::RecoveryConflict
        );
        assert_eq!(fs::read(journal_path).unwrap(), original_journal);
    }

    #[test]
    fn target_repair_transaction_recovers_interrupted_prepared_journal_before_admission() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let changes = vec![
            test_change(
                StoreFile::Actions,
                root,
                b"actions-before",
                b"actions-after",
            ),
            test_change(
                StoreFile::Folders,
                root,
                b"folders-before",
                b"folders-after",
            ),
        ];
        let writes = AtomicU64::new(0);
        let result = persist_transaction(
            root,
            &changes,
            disk_reader,
            |path, bytes| {
                let index = writes.fetch_add(1, Ordering::AcqRel);
                match index {
                    0 => fs::write(path, bytes).map_err(|_| ()),
                    1 => {
                        // Simulate an external edit racing rollback so the
                        // prepared journal must survive until startup recovery.
                        fs::write(&changes[0].path, b"external edit").map_err(|_| ())?;
                        Err(())
                    }
                    _ => fs::write(path, bytes).map_err(|_| ()),
                }
            },
            None,
            None,
        );
        assert_eq!(
            result.err().unwrap().kind,
            RepairFailureKind::RecoveryConflict
        );
        assert!(root.join(JOURNAL_FILE).exists());

        let paths = store_paths(root)
            .into_iter()
            .map(|(_, path)| path)
            .collect::<Vec<_>>();
        crate::common::repair_watch::block_paths(paths.clone());
        assert!(crate::common::repair_watch::try_read_path(&changes[0].path).is_none());

        // Once the affected file again exactly matches the journal's written
        // bytes, startup can safely restore the pre-transaction snapshot.
        fs::write(&changes[0].path, b"actions-after").unwrap();
        recover_startup(root).unwrap();

        assert_eq!(fs::read(&changes[0].path).unwrap(), b"actions-before");
        assert_eq!(fs::read(&changes[1].path).unwrap(), b"folders-before");
        assert!(!root.join(JOURNAL_FILE).exists());
        assert!(crate::common::repair_watch::try_read_path(&changes[0].path).is_some());
    }

    #[test]
    fn target_repair_transaction_rejects_usage_generation_change_before_commit() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let change = test_change(StoreFile::Actions, root, b"before", b"after");
        let revision = Arc::new(AtomicU64::new(1));
        let barrier = Arc::new(Mutex::new(()));
        let snapshot = LiveUsageSnapshot::capture(
            &HashMap::from([("old".to_owned(), 3)]),
            Arc::clone(&revision),
            Arc::clone(&barrier),
            Arc::new(Mutex::new(HashMap::new())),
            UsageHandoff::default(),
        )
        .unwrap();
        revision.fetch_add(1, Ordering::AcqRel);
        let result = persist_transaction(
            root,
            std::slice::from_ref(&change),
            disk_reader,
            write_store,
            Some(&snapshot),
            None,
        );
        assert_eq!(
            result.err().unwrap().kind,
            RepairFailureKind::DestinationCollision
        );
        assert_eq!(fs::read(change.path).unwrap(), b"before");
        assert!(!root.join(JOURNAL_FILE).exists());
    }

    #[test]
    fn target_repair_transaction_defers_usage_deltas_without_waiting_on_commit_barrier() {
        let barrier = Mutex::new(());
        let deferred = Mutex::new(HashMap::new());
        let generation = AtomicU64::new(4);
        let mut usage = HashMap::from([("before".to_owned(), 3)]);
        let _commit_guard = barrier.lock().unwrap();

        record_usage_delta(
            "after",
            &mut usage,
            &deferred,
            &generation,
            &barrier,
            &UsageHandoff::default(),
        );

        assert_eq!(usage, HashMap::from([("before".to_owned(), 3)]));
        assert_eq!(generation.load(Ordering::Acquire), 5);
        assert_eq!(
            *deferred.lock().unwrap(),
            HashMap::from([("after".to_owned(), 1)])
        );
        let handoff = UsageHandoff::default();
        assert_eq!(
            handoff
                .begin(
                    3,
                    "before",
                    "after",
                    PathBuf::from("usage.json"),
                    &generation,
                    generation.load(Ordering::Acquire),
                    &deferred,
                )
                .err()
                .unwrap()
                .kind,
            RepairFailureKind::DestinationCollision
        );
        assert_eq!(
            *deferred.lock().unwrap(),
            HashMap::from([("after".to_owned(), 1)])
        );
    }

    #[test]
    fn target_repair_transaction_usage_handoff_is_exact_once_and_scoped_to_commit() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let usage_path = root.join(usage::USAGE_FILE);
        let usage_path_str = usage_path.to_string_lossy().into_owned();
        let handoff = UsageHandoff::default();
        let barrier = Arc::new(Mutex::new(()));
        let deferred = Arc::new(Mutex::new(HashMap::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let old = "C:\\missing\\counted.exe";
        let new = root.join("replacement.exe").to_string_lossy().into_owned();
        let mut usage_map = HashMap::from([(old.to_owned(), 5_u32)]);
        let operation = handoff
            .begin(80, old, &new, usage_path.clone(), &generation, 0, &deferred)
            .unwrap();
        operation.mark_committed();

        // These increments arrive after disk commit but before the GUI or
        // shutdown owner acknowledges the operation receipt.
        record_usage_delta(
            old,
            &mut usage_map,
            &deferred,
            &generation,
            &barrier,
            &handoff,
        );
        record_usage_delta(
            &new,
            &mut usage_map,
            &deferred,
            &generation,
            &barrier,
            &handoff,
        );
        assert_eq!(usage_map, HashMap::from([(old.to_owned(), 5)]));

        let app_root = AppDataRoot::from_path(root.to_path_buf());
        let radial = Arc::new(RadialStore::new(&app_root).unwrap());
        let hotfolder = HotFolderRulesStore::new(&app_root);
        let mut client = TargetRepairClient::new(
            root,
            radial,
            hotfolder,
            handoff.clone(),
            Arc::clone(&barrier),
            || {},
        )
        .unwrap();
        client
            .save_usage_on_exit(&usage_path_str, &mut usage_map, &deferred)
            .unwrap();
        assert_eq!(usage_map, HashMap::from([(new.clone(), 7)]));
        let persisted = usage::load_usage(&usage_path_str).unwrap();
        assert_eq!(persisted, HashMap::from([(new.clone(), 7)]));

        // A later execution of the old path belongs to a later operation;
        // the completed handoff does not leave a permanent redirect behind.
        record_usage_delta(
            old,
            &mut usage_map,
            &deferred,
            &generation,
            &barrier,
            &handoff,
        );
        client
            .save_usage_on_exit(&usage_path_str, &mut usage_map, &deferred)
            .unwrap();
        assert_eq!(usage_map, HashMap::from([(new, 7), (old.to_owned(), 1)]));
        assert_eq!(usage::load_usage(&usage_path_str).unwrap(), usage_map);
        client.shutdown();
    }

    #[test]
    fn target_repair_transaction_exit_waits_for_committed_repair_before_usage_save() {
        let _actions_test_guard = ACTION_REPAIR_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let original_path = root
            .join("missing-source.exe")
            .to_string_lossy()
            .into_owned();
        let replacement = root.join("replacement.exe");
        fs::write(&replacement, b"replacement target").unwrap();
        let original = action(&original_path, None);
        write_documents(
            root,
            std::slice::from_ref(&original),
            &[],
            &[],
            &[],
            &[UsageEntry {
                action: original_path.clone(),
                count: 5,
            }],
        );

        let app_root = AppDataRoot::from_path(root.to_path_buf());
        let radial = Arc::new(RadialStore::new(&app_root).unwrap());
        let hotfolder = HotFolderRulesStore::new(&app_root);
        let handoff = UsageHandoff::default();
        let barrier = Arc::new(Mutex::new(()));
        let generation = Arc::new(AtomicU64::new(0));
        let deferred = Arc::new(Mutex::new(HashMap::new()));
        let usage_map = HashMap::from([(original_path.clone(), 5_u32)]);
        let snapshot = LiveUsageSnapshot::capture(
            &usage_map,
            Arc::clone(&generation),
            Arc::clone(&barrier),
            Arc::clone(&deferred),
            handoff.clone(),
        )
        .unwrap();
        let request = RepairRequest::new(SelectedTarget::action(original, 0), replacement.clone())
            .with_live_usage(snapshot);
        let usage_path = root.join(usage::USAGE_FILE);
        let usage_path_str = usage_path.to_string_lossy().into_owned();
        let client =
            TargetRepairClient::new(root, radial, hotfolder, handoff, barrier, || {}).unwrap();
        let (wait_started_tx, wait_started_rx) = mpsc::channel();
        client.lifecycle.observe_next_wait(wait_started_tx);
        let (repair_started_tx, repair_started_rx) = mpsc::channel();
        client.lifecycle.observe_next_start(repair_started_tx);
        let usage_path_for_exit = usage_path_str.clone();

        // Hold the coordinator before its first owner lock. Once exit reports
        // that it is waiting for admitted work, releasing this guard lets the
        // queued repair commit and resolve its path fence deterministically.
        let repair_guard = REPAIR_TRANSACTION
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        client.submit(request).unwrap();
        repair_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the worker has claimed the queued repair before exit");
        let exit = thread::spawn(move || {
            let mut client = client;
            let mut usage_map = usage_map;
            client.shutdown();
            let result = client.save_usage_on_exit(&usage_path_for_exit, &mut usage_map, &deferred);
            (result, usage_map)
        });
        wait_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("exit waits for the admitted repair before saving usage");
        drop(repair_guard);

        let (save_result, usage_map) = exit.join().expect("exit owner completes");
        save_result.unwrap();
        let replacement_path = replacement.to_string_lossy().into_owned();
        assert_eq!(usage_map, HashMap::from([(replacement_path.clone(), 5)]));
        assert_eq!(
            usage::load_usage(&usage_path_str).unwrap(),
            HashMap::from([(replacement_path, 5)])
        );
        assert!(!usage_map.contains_key(&original_path));
    }

    #[test]
    fn target_repair_transaction_maps_zero_base_deltas_and_replays_restored_deltas() {
        let handoff = UsageHandoff::default();
        let generation = AtomicU64::new(0);
        let barrier = Mutex::new(());
        let deferred = Mutex::new(HashMap::new());
        let mut usage_map = HashMap::new();
        let old = "C:\\missing\\not-yet-counted.exe";
        let new = "D:\\replacement\\not-yet-counted.exe";
        let operation = handoff
            .begin(
                90,
                old,
                new,
                PathBuf::from("usage.json"),
                &generation,
                0,
                &deferred,
            )
            .unwrap();
        record_usage_delta(
            old,
            &mut usage_map,
            &deferred,
            &generation,
            &barrier,
            &handoff,
        );
        record_usage_delta(
            "unrelated",
            &mut usage_map,
            &deferred,
            &generation,
            &barrier,
            &handoff,
        );
        operation.mark_committed();
        let applied = handoff.apply_ready(&mut usage_map, &deferred);
        assert!(applied.changed);
        assert_eq!(
            usage_map,
            HashMap::from([(new.to_owned(), 1), ("unrelated".to_owned(), 1)])
        );
        assert!(!handoff.apply_ready(&mut usage_map, &deferred).changed);
        assert_eq!(usage_map[new], 1, "duplicate receipt is inert");

        let rollback = handoff
            .begin(
                91,
                old,
                new,
                PathBuf::from("usage.json"),
                &generation,
                0,
                &deferred,
            )
            .unwrap();
        record_usage_delta(
            old,
            &mut usage_map,
            &deferred,
            &generation,
            &barrier,
            &handoff,
        );
        rollback.mark_restored();
        handoff.apply_ready(&mut usage_map, &deferred);
        assert_eq!(usage_map[old], 1);
        assert_eq!(usage_map[new], 1);
    }

    struct RecoveryBlockCleanup(Vec<PathBuf>);

    impl Drop for RecoveryBlockCleanup {
        fn drop(&mut self) {
            crate::common::repair_watch::resolve_recovered_paths(std::mem::take(&mut self.0));
        }
    }

    #[test]
    fn target_repair_transaction_recovery_conflict_blocks_owner_mutations_and_usage_exit() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        write_documents(root, &[], &[], &[], &[], &[]);
        let app_root = AppDataRoot::from_path(root.to_path_buf());
        let folders_path = root.join(StoreFile::Folders.file_name());
        let folders_path_str = folders_path.to_string_lossy().into_owned();
        let hotfolder = HotFolderRulesStore::new(&app_root);
        hotfolder.load();
        let usage_path = root.join(StoreFile::Usage.file_name());
        let usage_path_str = usage_path.to_string_lossy().into_owned();
        let folder_bytes = fs::read(&folders_path).unwrap();
        let usage_bytes = fs::read(&usage_path).unwrap();
        let hotfolder_missing = !hotfolder.path().exists();
        let paths = repair_fence_paths(root);
        crate::common::repair_watch::block_paths(paths.clone());
        let _block_cleanup = RecoveryBlockCleanup(paths);

        assert!(
            crate::plugins::folders::update_folders(&folders_path_str, |folders| {
                folders.push(FolderEntry {
                    label: "must not appear".into(),
                    path: root.join("unrelated").to_string_lossy().into_owned(),
                    alias: None,
                });
                Ok(true)
            })
            .is_err()
        );
        assert_eq!(fs::read(&folders_path).unwrap(), folder_bytes);

        let error = hotfolder.add_rule(
            0,
            crate::hotfolder::HotFolderRule::new(
                "must not appear",
                root.join("watch"),
                crate::hotfolder::RuleAction::Copy {
                    destination: root.join("destination"),
                },
            ),
        );
        assert!(matches!(
            error,
            Err(crate::hotfolder::HotFolderStoreError::WriteBlocked(_))
        ));
        assert_eq!(hotfolder.path().exists(), !hotfolder_missing);

        let handoff = UsageHandoff::default();
        let generation = AtomicU64::new(0);
        let deferred = Mutex::new(HashMap::new());
        let operation = handoff
            .begin(
                100,
                "C:\\old.exe",
                "D:\\new.exe",
                usage_path.clone(),
                &generation,
                0,
                &deferred,
            )
            .unwrap();
        operation.mark_recovery_conflict();
        let radial = Arc::new(RadialStore::new(&app_root).unwrap());
        let mut client = TargetRepairClient::new(
            root,
            radial,
            hotfolder,
            handoff,
            Arc::new(Mutex::new(())),
            || {},
        )
        .unwrap();
        let mut usage_map = HashMap::from([("C:\\old.exe".to_owned(), 4)]);
        assert!(
            client
                .save_usage_on_exit(&usage_path_str, &mut usage_map, &deferred)
                .is_err()
        );
        assert_eq!(fs::read(&usage_path).unwrap(), usage_bytes);
        client.shutdown();
    }
}
