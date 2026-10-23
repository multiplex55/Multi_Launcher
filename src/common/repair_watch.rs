//! Exact-path read permits and watcher deferral for target-repair transactions.
//!
//! A repair may publish several JSON stores together. Readers of any changed
//! path must wait until the transaction commits or a complete rollback is
//! verified. If rollback cannot be verified, that path stays blocked until
//! startup recovery resolves the durable journal.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Condvar, LazyLock, Mutex,
    atomic::{AtomicU64, Ordering},
};

type DeferredCallback = Box<dyn FnOnce() + Send + 'static>;

#[derive(Default)]
struct FenceEntry {
    depth: usize,
    blocked: bool,
    active_readers: usize,
    callbacks: HashMap<u64, DeferredCallback>,
}

#[derive(Default)]
struct RepairWatchRegistry {
    entries: Mutex<HashMap<PathBuf, FenceEntry>>,
    readers_drained: Condvar,
}

static FENCES: LazyLock<RepairWatchRegistry> = LazyLock::new(RepairWatchRegistry::default);
static NEXT_SUBSCRIBER: AtomicU64 = AtomicU64::new(1);

/// Stable identity for one watcher subscription. Deferred notifications from
/// the same subscription are coalesced to its newest callback.
pub(crate) fn new_subscriber_id() -> u64 {
    NEXT_SUBSCRIBER.fetch_add(1, Ordering::Relaxed)
}

/// The GUI has one coalesced event subscription per repairable owner path.
/// Reserve a separate ID range from independently registered JSON watchers.
pub(crate) fn subscriber_for_event(event: &crate::gui::WatchEvent) -> u64 {
    let tag = match event {
        crate::gui::WatchEvent::Actions => 1,
        crate::gui::WatchEvent::Folders => 2,
        crate::gui::WatchEvent::Favorites => 3,
        crate::gui::WatchEvent::Gestures => 4,
        _ => 0,
    };
    (1_u64 << 63) | tag
}

pub(crate) struct RepairWatchFence {
    paths: Vec<PathBuf>,
    transaction_started: bool,
    resolved: bool,
}

impl RepairWatchFence {
    /// Acquires all path fences atomically. An unresolved prior rollback
    /// conflict refuses a new transaction rather than replacing its journal.
    /// Already-admitted readers finish before the caller may mutate stores;
    /// reserving depth first prevents new readers from entering while waiting.
    pub(crate) fn new(paths: impl IntoIterator<Item = PathBuf>) -> Result<Self, Vec<PathBuf>> {
        Self::new_inner(paths, || {})
    }

    fn new_inner(
        paths: impl IntoIterator<Item = PathBuf>,
        reserved: impl FnOnce(),
    ) -> Result<Self, Vec<PathBuf>> {
        let mut paths = paths.into_iter().map(normalize).collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        let mut fences = FENCES
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let blocked = paths
            .iter()
            .filter(|path| fences.get(*path).is_some_and(|entry| entry.blocked))
            .cloned()
            .collect::<Vec<_>>();
        if !blocked.is_empty() {
            return Err(blocked);
        }
        for path in &paths {
            fences.entry(path.clone()).or_default().depth += 1;
        }
        reserved();
        while paths.iter().any(|path| {
            fences
                .get(path)
                .is_some_and(|entry| entry.active_readers > 0)
        }) {
            fences = FENCES
                .readers_drained
                .wait(fences)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        Ok(Self {
            paths,
            transaction_started: false,
            resolved: false,
        })
    }

    /// From this point, dropping without `resolve` means the on-disk outcome
    /// is uncertain and the exact paths must remain blocked.
    pub(crate) fn mark_transaction_started(&mut self) {
        self.transaction_started = true;
    }

    /// Releases the fence after commit or a verified full rollback.
    pub(crate) fn resolve(&mut self) {
        self.resolved = true;
    }
}

impl Drop for RepairWatchFence {
    fn drop(&mut self) {
        let callbacks = {
            let mut fences = FENCES
                .entries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut callbacks = Vec::new();
            for path in &self.paths {
                let Some(entry) = fences.get_mut(path) else {
                    continue;
                };
                if self.transaction_started && !self.resolved {
                    entry.blocked = true;
                }
                entry.depth = entry.depth.saturating_sub(1);
                if entry.depth == 0 && !entry.blocked {
                    for callback in std::mem::take(&mut entry.callbacks).into_values() {
                        entry.active_readers = entry.active_readers.saturating_add(1);
                        callbacks.push((path.clone(), callback));
                    }
                    if entry.active_readers == 0 {
                        fences.remove(path);
                    }
                }
            }
            callbacks
        };
        // Readers and notification callbacks may acquire owner locks. Never
        // invoke them while holding the registry mutex.
        for (path, callback) in callbacks {
            let _permit = PathReadPermit { path };
            callback();
        }
    }
}

/// A read permit prevents a repair from starting between the fence check and
/// the caller's synchronous read/publication without holding the registry
/// mutex across owner or filesystem work.
pub(crate) struct PathReadPermit {
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathReadRefusal {
    RepairInProgress,
    RecoveryConflict,
}

impl Drop for PathReadPermit {
    fn drop(&mut self) {
        {
            let mut fences = FENCES
                .entries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = fences.get_mut(&self.path) {
                entry.active_readers = entry.active_readers.saturating_sub(1);
                if entry.active_readers == 0
                    && entry.depth == 0
                    && !entry.blocked
                    && entry.callbacks.is_empty()
                {
                    fences.remove(&self.path);
                }
            }
        }
        FENCES.readers_drained.notify_all();
    }
}

pub(crate) fn try_read_path(path: &Path) -> Option<PathReadPermit> {
    try_read_path_checked(path).ok()
}

/// Acquires a read/mutation permit while preserving the reason for refusal.
/// Owner APIs use this before taking their transaction lock so an unresolved
/// repair never exposes or republishes a partial store.
pub(crate) fn try_read_path_checked(path: &Path) -> Result<PathReadPermit, PathReadRefusal> {
    let mut fences = FENCES
        .entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = normalize(path.to_path_buf());
    let entry = fences.entry(key.clone()).or_default();
    if entry.blocked {
        return Err(PathReadRefusal::RecoveryConflict);
    }
    if entry.depth > 0 {
        return Err(PathReadRefusal::RepairInProgress);
    }
    entry.active_readers = entry.active_readers.saturating_add(1);
    Ok(PathReadPermit { path: key })
}

pub(crate) fn require_path_read(path: &Path) -> anyhow::Result<PathReadPermit> {
    try_read_path_checked(path).map_err(|refusal| match refusal {
        PathReadRefusal::RepairInProgress => {
            anyhow::anyhow!("saved-target repair is in progress; retry this operation")
        }
        PathReadRefusal::RecoveryConflict => {
            anyhow::anyhow!("saved-target recovery needs attention; this store is unavailable")
        }
    })
}

pub(crate) fn require_persisted_read(
    path: &Path,
) -> Result<PathReadPermit, crate::common::persistence::PersistenceError> {
    try_read_path_checked(path).map_err(|refusal| {
        crate::common::persistence::PersistenceError::Read {
            path: path.to_path_buf(),
            source: std::io::Error::other(match refusal {
                PathReadRefusal::RepairInProgress => {
                    "saved-target repair is in progress; retry this read"
                }
                PathReadRefusal::RecoveryConflict => {
                    "saved-target recovery needs attention; this store is unavailable"
                }
            }),
        }
    })
}

/// Run a watcher callback now, or retain the newest callback for this exact
/// subscriber until the path is released. Returns true when it ran now.
pub(crate) fn dispatch_or_defer_path(
    path: &Path,
    subscriber_id: u64,
    callback: impl FnOnce() + Send + 'static,
) -> bool {
    let key = normalize(path.to_path_buf());
    let mut callback: Option<DeferredCallback> = Some(Box::new(callback));
    let run_now = {
        let mut fences = FENCES
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = fences.entry(key.clone()).or_default();
        if entry.depth > 0 || entry.blocked {
            entry
                .callbacks
                .insert(subscriber_id, callback.take().expect("callback present"));
            false
        } else {
            entry.active_readers = entry.active_readers.saturating_add(1);
            true
        }
    };
    if run_now {
        let callback = callback.expect("callback is retained only when deferred");
        let _permit = PathReadPermit { path: key };
        callback();
        true
    } else {
        false
    }
}

/// Clears a recovery-conflict block after the durable journal has been
/// successfully resolved. Deferred callbacks run only after the registry
/// lock is released.
pub(crate) fn resolve_recovered_paths(paths: impl IntoIterator<Item = PathBuf>) {
    let callbacks = {
        let mut fences = FENCES
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut callbacks = Vec::new();
        for path in paths.into_iter().map(normalize) {
            let Some(entry) = fences.get_mut(&path) else {
                continue;
            };
            entry.blocked = false;
            if entry.depth == 0 {
                for callback in std::mem::take(&mut entry.callbacks).into_values() {
                    entry.active_readers = entry.active_readers.saturating_add(1);
                    callbacks.push((path.clone(), callback));
                }
                if entry.active_readers == 0 {
                    fences.remove(&path);
                }
            }
        }
        callbacks
    };
    for (path, callback) in callbacks {
        let _permit = PathReadPermit { path };
        callback();
    }
}

pub(crate) fn block_paths(paths: impl IntoIterator<Item = PathBuf>) {
    let mut fences = FENCES
        .entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for path in paths.into_iter().map(normalize) {
        fences.entry(path).or_default().blocked = true;
    }
}

pub(crate) fn any_blocked(paths: impl IntoIterator<Item = PathBuf>) -> bool {
    let fences = FENCES
        .entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    paths
        .into_iter()
        .map(normalize)
        .any(|path| fences.get(&path).is_some_and(|entry| entry.blocked))
}

fn normalize(path: PathBuf) -> PathBuf {
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
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn target_repair_plan_watchers_wait_until_the_exact_path_fence_releases() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("actions.json");
        let called = Arc::new(AtomicUsize::new(0));
        {
            let mut fence = RepairWatchFence::new([path.clone()]).unwrap();
            fence.mark_transaction_started();
            let called_in_callback = Arc::clone(&called);
            assert!(!dispatch_or_defer_path(
                &path,
                new_subscriber_id(),
                move || {
                    called_in_callback.fetch_add(1, Ordering::SeqCst);
                },
            ));
            assert!(try_read_path(&path).is_none());
            fence.resolve();
        }
        assert_eq!(called.load(Ordering::SeqCst), 1);
        assert!(try_read_path(&path).is_some());
    }

    #[test]
    fn target_repair_transaction_conflict_coalesces_and_blocks_until_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("actions.json");
        let subscriber = new_subscriber_id();
        let called = Arc::new(AtomicUsize::new(0));
        {
            let mut fence = RepairWatchFence::new([path.clone()]).unwrap();
            fence.mark_transaction_started();
            for _ in 0..20 {
                let called_in_callback = Arc::clone(&called);
                assert!(!dispatch_or_defer_path(&path, subscriber, move || {
                    called_in_callback.fetch_add(1, Ordering::SeqCst);
                }));
            }
            assert!(try_read_path(&path).is_none());
            // An uncertain rollback intentionally drops unresolved.
        }
        assert_eq!(called.load(Ordering::SeqCst), 0);
        assert!(try_read_path(&path).is_none());
        assert!(RepairWatchFence::new([path.clone()]).is_err());
        resolve_recovered_paths([path.clone()]);
        assert_eq!(called.load(Ordering::SeqCst), 1);
        assert!(try_read_path(&path).is_some());
        assert!(RepairWatchFence::new([path]).is_ok());
    }

    #[test]
    fn target_repair_transaction_waits_for_admitted_reader_before_mutation() {
        use std::sync::mpsc;
        use std::thread;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("actions.json");
        let subscriber = new_subscriber_id();
        let owner = Arc::new(Mutex::new(()));
        let owner_for_reader = Arc::clone(&owner);
        let (reader_entered_tx, reader_entered_rx) = mpsc::channel();
        let (release_reader_tx, release_reader_rx) = mpsc::channel();
        let reader_path = path.clone();
        let reader = thread::spawn(move || {
            assert!(dispatch_or_defer_path(
                &reader_path,
                subscriber,
                move || {
                    let _owner = owner_for_reader.lock().unwrap();
                    reader_entered_tx.send(()).unwrap();
                    release_reader_rx.recv().unwrap();
                }
            ));
        });
        reader_entered_rx.recv().unwrap();

        let (reserved_tx, reserved_rx) = mpsc::channel();
        let (fence_done_tx, fence_done_rx) = mpsc::channel();
        let owner_for_repair = Arc::clone(&owner);
        let repair_path = path.clone();
        let repair = thread::spawn(move || {
            let mut fence = RepairWatchFence::new_inner([repair_path.clone()], || {
                reserved_tx.send(()).unwrap();
            })
            .unwrap();
            let _owner = owner_for_repair.lock().unwrap();
            fence.mark_transaction_started();

            let read_after_commit = fence_done_tx.clone();
            assert!(!dispatch_or_defer_path(
                &repair_path,
                subscriber,
                move || {
                    read_after_commit
                        .send("deferred reader observed committed data")
                        .unwrap();
                },
            ));
            fence.resolve();
            drop(fence);
            fence_done_tx.send("repair completed").unwrap();
        });

        reserved_rx.recv().unwrap();
        assert!(
            fence_done_rx.try_recv().is_err(),
            "a reserved writer must not pass an already-admitted reader"
        );
        assert!(try_read_path(&path).is_none());
        release_reader_tx.send(()).unwrap();
        reader.join().unwrap();
        assert_eq!(
            fence_done_rx.recv().unwrap(),
            "deferred reader observed committed data"
        );
        assert_eq!(fence_done_rx.recv().unwrap(), "repair completed");
        repair.join().unwrap();
    }
}
