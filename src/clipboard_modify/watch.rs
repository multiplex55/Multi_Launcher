use super::store::ClipboardModifierStore;
use crate::gui::ClipboardModifyGuiEvent;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A UI-owned configuration watcher.
///
/// Filesystem callbacks only enqueue notifications. Debouncing and loading are
/// deliberately performed by [`poll`](Self::poll), so the regular GUI update
/// pass is the single place that mutates the live catalog and diagnostics.
pub struct ClipboardModifyWatcher {
    watcher: RecommendedWatcher,
    path: PathBuf,
    store: ClipboardModifierStore,
    debounce: Duration,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    reload_deadline: Option<Instant>,
}

impl ClipboardModifyWatcher {
    pub fn start(
        store: ClipboardModifierStore,
        debounce: Duration,
        repaint: impl Fn() + Send + 'static,
    ) -> notify::Result<Self> {
        let path = store.path.clone();
        let watch_path = path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let (tx, rx) = mpsc::channel();
        #[cfg(test)]
        let release = crate::performance::track_c_watcher_lifecycle::capture(
            crate::performance::track_c_watcher_lifecycle::HandlerKind::ClipboardModify,
            &watch_path,
        );
        #[cfg(test)]
        let release_registration = release.as_ref().map(|sentinel| sentinel.registration());
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                #[cfg(test)]
                let _keep_handler_release = &release;
                if tx.send(res).is_ok() {
                    repaint();
                }
            },
            Config::default(),
        )?;
        watcher.watch(&watch_path, RecursiveMode::NonRecursive)?;
        #[cfg(test)]
        if let Some(registration) = release_registration {
            registration.registered();
        }
        Ok(Self {
            watcher,
            path,
            store,
            debounce,
            rx,
            reload_deadline: None,
        })
    }

    /// Drain notifications, advance the debounce deadline, and perform at most
    /// one reload. Returned events contain status/diagnostics only, never file
    /// or clipboard-derived text.
    pub fn poll(&mut self, now: Instant) -> Vec<ClipboardModifyGuiEvent> {
        while let Ok(result) = self.rx.try_recv() {
            match result {
                Ok(event)
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) && crate::common::json_watch::event_targets_path(
                        &event, &self.path, false,
                    ) =>
                {
                    self.reload_deadline = Some(now + self.debounce);
                }
                Ok(_) => {}
                Err(err) => {
                    return vec![ClipboardModifyGuiEvent::ConfigurationReloadFailure(
                        err.to_string(),
                    )];
                }
            }
        }
        if self.reload_deadline.is_none_or(|deadline| now < deadline) {
            return Vec::new();
        }
        self.reload_deadline = None;
        let event = match self.store.reload_now() {
            Ok(_) => ClipboardModifyGuiEvent::ConfigurationReloadSuccess,
            Err(error) => {
                let error = error.to_string();
                ClipboardModifyGuiEvent::ConfigurationReloadFailure(error)
            }
        };
        vec![event]
    }

    pub fn has_pending_reload(&self) -> bool {
        self.reload_deadline.is_some()
    }
}

impl Drop for ClipboardModifyWatcher {
    fn drop(&mut self) {
        // Explicitly touch the handle: dropping it unregisters the OS watcher.
        let _ = &self.watcher;
    }
}
