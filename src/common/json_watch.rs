use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

type Callback = Arc<Mutex<Box<dyn FnMut() + Send>>>;

/// Handle to a registered JSON file watcher.
/// Dropping the handle removes the associated callback.
#[derive(Debug)]
pub struct JsonWatcher {
    path: PathBuf,
    id: usize,
}

struct WatchEntry {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    callbacks: Arc<Mutex<HashMap<usize, Callback>>>,
}

static WATCHERS: Lazy<Mutex<HashMap<PathBuf, WatchEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

impl Drop for JsonWatcher {
    fn drop(&mut self) {
        if let Ok(mut map) = WATCHERS.lock()
            && let Some(entry) = map.get_mut(&self.path)
        {
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

/// Watch a JSON file and invoke `callback` whenever it changes.
///
/// Returns a handle that must be kept alive for the callbacks to trigger.
pub fn watch_json<F, P>(path: P, callback: F) -> notify::Result<JsonWatcher>
where
    F: FnMut() + Send + 'static,
    P: AsRef<Path>,
{
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let path_buf = normalized_path(path.as_ref());

    let mut map = WATCHERS.lock().unwrap();
    if let Some(entry) = map.get_mut(&path_buf) {
        entry
            .callbacks
            .lock()
            .unwrap()
            .insert(id, Arc::new(Mutex::new(Box::new(callback))));
        return Ok(JsonWatcher { path: path_buf, id });
    }

    let callbacks: Arc<Mutex<HashMap<usize, Callback>>> = Arc::new(Mutex::new(HashMap::new()));
    callbacks
        .lock()
        .unwrap()
        .insert(id, Arc::new(Mutex::new(Box::new(callback))));
    let callbacks_clone = callbacks.clone();
    let target = path_buf.clone();

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) && event_targets_path(&event, &target, false)
                {
                    invoke_callbacks(&callbacks_clone);
                }
            }
            Err(e) => tracing::error!("watch error: {:?}", e),
        },
        Config::default(),
    )?;

    // Always watch the parent: atomic replacement swaps a sibling temporary
    // file into the target and can detach a watcher registered on the old inode.
    let parent = path_buf.parent().unwrap_or_else(|| Path::new("."));
    watcher.watch(parent, RecursiveMode::NonRecursive)?;

    map.insert(path_buf.clone(), WatchEntry { watcher, callbacks });

    Ok(JsonWatcher { path: path_buf, id })
}

fn invoke_callbacks(callbacks: &Arc<Mutex<HashMap<usize, Callback>>>) {
    let callbacks = callbacks
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for callback in callbacks {
        if let Ok(mut callback) = callback.lock() {
            callback();
        }
    }
}

pub(crate) fn event_targets_path(
    event: &notify::Event,
    target: &Path,
    target_is_directory: bool,
) -> bool {
    let target = normalized_path(target);
    event.paths.iter().any(|event_path| {
        let event_path = normalized_path(event_path);
        if target_is_directory {
            event_path.starts_with(&target)
        } else {
            event_path == target
        }
    })
}

fn normalized_path(path: &Path) -> PathBuf {
    use std::path::Component;
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
        let normalized = normalized_path(&path);
        assert!(WATCHERS.lock().unwrap().contains_key(&normalized));

        drop(w1);
        assert!(WATCHERS.lock().unwrap().contains_key(&normalized));
        drop(w2);

        assert!(!WATCHERS.lock().unwrap().contains_key(&normalized));
    }

    #[test]
    fn exact_target_filter_ignores_siblings_and_accepts_atomic_target_path() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("target.json");
        let sibling = dir.path().join("sibling.json");
        let target_event = notify::Event::new(EventKind::Create(notify::event::CreateKind::File))
            .add_path(target.clone());
        let sibling_event =
            notify::Event::new(EventKind::Modify(notify::event::ModifyKind::Any)).add_path(sibling);

        assert!(event_targets_path(&target_event, &target, false));
        assert!(!event_targets_path(&sibling_event, &target, false));
    }

    #[test]
    fn callbacks_are_invoked_without_holding_the_callback_registry_lock() {
        let callbacks: Arc<Mutex<HashMap<usize, Callback>>> = Arc::new(Mutex::new(HashMap::new()));
        let registry = Arc::clone(&callbacks);
        callbacks.lock().unwrap().insert(
            1,
            Arc::new(Mutex::new(Box::new(move || {
                assert!(registry.try_lock().is_ok());
            }))),
        );

        invoke_callbacks(&callbacks);
    }

    #[test]
    fn parent_watcher_observes_atomic_target_replacement() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("target.json");
        std::fs::write(&path, "{}").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let _watcher = watch_json(&path, move || {
            let _ = tx.send(());
        })
        .unwrap();

        crate::common::persistence::save_json_atomic(&path, &serde_json::json!({"v": 2})).unwrap();

        rx.recv_timeout(std::time::Duration::from_secs(3))
            .expect("atomic replacement should notify the exact target watcher");
    }
}
