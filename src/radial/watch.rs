//! Main-owner filesystem notification adapter for radial configuration.
//!
//! `notify` owns the platform callback thread. This adapter only coalesces its
//! callbacks and wakes the existing main event loop; it does not create a
//! second persistence owner or polling daemon.

use super::model::{RADIAL_ASSETS_DIRECTORY, RADIAL_FILE};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc::Sender};
use std::time::Duration;

const DOCUMENT_DIRTY: u8 = 1;
const ASSETS_DIRTY: u8 = 2;
const DEBOUNCE: Duration = Duration::from_millis(75);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RadialWatchChanges {
    pub document: bool,
    pub assets: bool,
}

impl RadialWatchChanges {
    pub fn is_empty(self) -> bool {
        !self.document && !self.assets
    }
}

#[derive(Default)]
struct PendingState {
    bits: u8,
    wake_scheduled: bool,
}

#[derive(Default)]
struct PendingChanges(Mutex<PendingState>);

impl PendingChanges {
    fn record(&self, changes: RadialWatchChanges) -> bool {
        let mut bits = 0;
        if changes.document {
            bits |= DOCUMENT_DIRTY;
        }
        if changes.assets {
            bits |= ASSETS_DIRTY;
        }
        if bits == 0 {
            return false;
        }
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.bits |= bits;
        let schedule = !state.wake_scheduled;
        state.wake_scheduled = true;
        schedule
    }

    fn take(&self) -> RadialWatchChanges {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let bits = state.bits;
        state.bits = 0;
        state.wake_scheduled = false;
        RadialWatchChanges {
            document: bits & DOCUMENT_DIRTY != 0,
            assets: bits & ASSETS_DIRTY != 0,
        }
    }
}

pub struct RadialConfigWatcher {
    _watcher: RecommendedWatcher,
    pending: Arc<PendingChanges>,
}

impl RadialConfigWatcher {
    pub fn start(root: &Path, wake: Sender<()>) -> notify::Result<Self> {
        let radial_file = root.join(RADIAL_FILE);
        let asset_root = root.join(RADIAL_ASSETS_DIRECTORY);
        let pending = Arc::new(PendingChanges::default());
        let callback_pending = Arc::clone(&pending);
        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event)
                    if matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    ) =>
                {
                    let changes = classify_paths(&event.paths, &radial_file, &asset_root);
                    if callback_pending.record(changes) {
                        // Coalesce editor temp-file/rename storms on notify's existing
                        // callback worker, then wake the main owner exactly once.
                        std::thread::sleep(DEBOUNCE);
                        let _ = wake.send(());
                    }
                }
                Ok(_) => {}
                Err(error) => tracing::error!(%error, "radial config watch failed"),
            },
            Config::default(),
        )?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            pending,
        })
    }

    pub fn take(&self) -> RadialWatchChanges {
        self.pending.take()
    }
}

fn classify_paths(paths: &[PathBuf], radial_file: &Path, asset_root: &Path) -> RadialWatchChanges {
    RadialWatchChanges {
        document: paths.iter().any(|path| path == radial_file),
        assets: paths.iter().any(|path| path.starts_with(asset_root)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_is_scoped_to_document_and_asset_tree() {
        let root = Path::new(r"C:\profile");
        let document = root.join(RADIAL_FILE);
        let assets = root.join(RADIAL_ASSETS_DIRECTORY);
        assert_eq!(
            classify_paths(
                &[document.clone(), assets.join("skin/a.png")],
                &document,
                &assets
            ),
            RadialWatchChanges {
                document: true,
                assets: true
            }
        );
        assert!(classify_paths(&[root.join("settings.json")], &document, &assets).is_empty());
    }

    #[test]
    fn pending_notifications_are_coalesced_and_drained_atomically() {
        let pending = PendingChanges::default();
        assert!(pending.record(RadialWatchChanges {
            document: true,
            assets: false
        }));
        assert!(!pending.record(RadialWatchChanges {
            document: false,
            assets: true
        }));
        assert_eq!(
            pending.take(),
            RadialWatchChanges {
                document: true,
                assets: true
            }
        );
        assert!(pending.take().is_empty());
        assert!(pending.record(RadialWatchChanges {
            document: true,
            assets: false
        }));
    }
}
