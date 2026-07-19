use super::config::{LoadError, load_current_or_migrate};
use super::store::ClipboardModifierStore;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

pub struct ClipboardModifyWatcher {
    _watcher: RecommendedWatcher,
}

impl ClipboardModifyWatcher {
    pub fn start(store: ClipboardModifierStore, debounce: Duration) -> notify::Result<Self> {
        let path = store.path.clone();
        let watch_path = if path.exists() {
            path.clone()
        } else {
            path.parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf()
        };
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )?;
        watcher.watch(&watch_path, RecursiveMode::NonRecursive)?;
        std::thread::spawn(move || event_loop(path, store, debounce, rx));
        Ok(Self { _watcher: watcher })
    }
}

fn event_loop(
    path: PathBuf,
    store: ClipboardModifierStore,
    debounce: Duration,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
) {
    let seq = AtomicU64::new(0);
    while let Ok(res) = rx.recv() {
        if let Ok(ev) = res {
            if !matches!(
                ev.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) {
                continue;
            }
            let my = seq.fetch_add(1, Ordering::SeqCst) + 1;
            std::thread::sleep(debounce);
            if seq.load(Ordering::SeqCst) != my {
                continue;
            }
            match load_current_or_migrate(&path) {
                Ok((_m, c)) => store.replace_valid(c),
                Err(LoadError::Future(v)) => {
                    store.retain_with_error(format!("unsupported future schema {v}"))
                }
                Err(e) => store.retain_with_error(e.to_string()),
            }
        }
    }
}
