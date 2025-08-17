use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

/// Handle to a registered JSON file watcher.
/// Dropping the handle removes the associated callback.
pub struct JsonWatcher {
    path: PathBuf,
    id: usize,
}

struct WatchEntry {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    callbacks: Arc<Mutex<HashMap<usize, Box<dyn FnMut() + Send>>>>,
}

static WATCHERS: Lazy<Mutex<HashMap<PathBuf, WatchEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

impl Drop for JsonWatcher {
    fn drop(&mut self) {
        if let Ok(mut map) = WATCHERS.lock() {
            if let Some(entry) = map.get_mut(&self.path) {
                let mut cbs = entry.callbacks.lock().unwrap();
                cbs.remove(&self.id);
                let empty = cbs.is_empty();
                drop(cbs);
                if empty {
                    map.remove(&self.path);
                }
            }
        }
    }
}

/// Watch a JSON file and invoke `callback` whenever it changes.
///
/// Returns a handle that must be kept alive for the callbacks to trigger.
pub fn watch_json<F, P>(path: P, callback: F) -> notify::Result<JsonWatcher>
where
    F: FnMut() + Send + 'static,
    P: AsRef<Path>,
{
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let path_buf = path.as_ref().to_path_buf();

    let mut map = WATCHERS.lock().unwrap();
    if let Some(entry) = map.get_mut(&path_buf) {
        entry
            .callbacks
            .lock()
            .unwrap()
            .insert(id, Box::new(callback));
        return Ok(JsonWatcher { path: path_buf, id });
    }

    let callbacks: Arc<Mutex<HashMap<usize, Box<dyn FnMut() + Send>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    callbacks.lock().unwrap().insert(id, Box::new(callback));
    let callbacks_clone = callbacks.clone();

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let mut cbs = callbacks_clone.lock().unwrap();
                    for cb in cbs.values_mut() {
                        cb();
                    }
                }
            }
            Err(e) => tracing::error!("watch error: {:?}", e),
        },
        Config::default(),
    )?;

    if watcher
        .watch(&path_buf, RecursiveMode::NonRecursive)
        .is_err()
    {
        let parent = path_buf.parent().unwrap_or_else(|| Path::new("."));
        watcher.watch(parent, RecursiveMode::NonRecursive)?;
    }

    map.insert(path_buf.clone(), WatchEntry { watcher, callbacks });

    Ok(JsonWatcher { path: path_buf, id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn entry_removed_when_all_handles_dropped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("file.json");
        std::fs::write(&path, "{}").unwrap();

        // create two watchers for same file
        let w1 = watch_json(&path, || {}).unwrap();
        let w2 = watch_json(&path, || {}).unwrap();
        assert!(WATCHERS.lock().unwrap().contains_key(&path));

        drop(w1);
        assert!(WATCHERS.lock().unwrap().contains_key(&path));
        drop(w2);

        assert!(!WATCHERS.lock().unwrap().contains_key(&path));
    }
}
