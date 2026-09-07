use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Process-owned, last-good snapshots keyed by the persisted store path.
///
/// Disk remains authoritative for transactions. Successful local commits and
/// valid watcher reloads publish into the same snapshot so every plugin
/// instance observes committed data immediately.
pub(crate) struct LiveSnapshotRegistry<T> {
    snapshots: Mutex<HashMap<PathBuf, Arc<Mutex<Vec<T>>>>>,
}

impl<T: Clone> LiveSnapshotRegistry<T> {
    pub(crate) fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn get_or_create(&self, path: &str, initial: Option<Vec<T>>) -> Arc<Mutex<Vec<T>>> {
        let key = normalized_path(Path::new(path));
        let mut snapshots = self
            .snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(snapshot) = snapshots.get(&key) {
            if let Some(initial) = initial
                && let Ok(mut current) = snapshot.lock()
            {
                *current = initial;
            }
            return Arc::clone(snapshot);
        }

        let snapshot = Arc::new(Mutex::new(initial.unwrap_or_default()));
        snapshots.insert(key, Arc::clone(&snapshot));
        snapshot
    }

    pub(crate) fn publish(&self, path: &str, committed: &[T]) {
        let snapshot = self.get_or_create(path, None);
        *snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = committed.to_vec();
    }
}

fn normalized_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
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
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}
