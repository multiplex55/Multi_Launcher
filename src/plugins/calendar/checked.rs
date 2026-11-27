//! Checked profile-scoped Calendar snapshots for non-egui frontends.
use super::*;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn relevant_event(
    event: &notify::Event,
    target: &std::path::Path,
    directory: &std::path::Path,
) -> bool {
    event.need_rescan()
        || (matches!(
            event.kind,
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
        ) && (crate::common::json_watch::event_targets_path(event, target, false)
            || crate::common::json_watch::event_targets_path(event, directory, false)))
}

#[derive(Clone, Debug)]
pub struct CheckedCalendarSnapshot {
    pub events: Arc<[CalendarEvent]>,
    pub revision: u64,
    pub status: Option<String>,
    pub actionable: bool,
    source: Option<Arc<Vec<u8>>>,
}

pub struct CheckedCalendarOwner {
    events_path: PathBuf,
    state_path: PathBuf,
    active: AtomicBool,
    seen_loaded: AtomicBool,
    snapshot: RwLock<Arc<CheckedCalendarSnapshot>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    callback_gate: Mutex<()>,
    operation_gate: Mutex<()>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl CheckedCalendarOwner {
    /// Start on a worker. The profile root is watched recursively so an absent
    /// calendar directory can appear without creating it during startup.
    pub fn start(
        root: &crate::platform::app_data::AppDataRoot,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<Arc<Self>> {
        let owner = Arc::new(Self {
            events_path: root.path().join(CALENDAR_EVENTS_FILE),
            state_path: root.path().join(CALENDAR_STATE_FILE),
            active: AtomicBool::new(true),
            seen_loaded: AtomicBool::new(false),
            snapshot: RwLock::new(Arc::new(CheckedCalendarSnapshot {
                events: Arc::from([]),
                revision: 0,
                status: None,
                actionable: false,
                source: None,
            })),
            watcher: Mutex::new(None),
            callback_gate: Mutex::new(()),
            operation_gate: Mutex::new(()),
            wake,
        });
        let weak = Arc::downgrade(&owner);
        let target = owner.events_path.clone();
        let directory = target
            .parent()
            .expect("calendar path has parent")
            .to_path_buf();
        let watch_id = (1_u64 << 62) | crate::common::repair_watch::new_subscriber_id();
        let mut watcher = RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                let event = match event {
                    Ok(event) => event,
                    Err(error) => {
                        if let Some(owner) = weak.upgrade() {
                            let _gate = owner
                                .callback_gate
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            owner.watcher_error(error.to_string());
                        }
                        return;
                    }
                };
                if !relevant_event(&event, &target, &directory) {
                    return;
                }
                let target = target.clone();
                let weak = weak.clone();
                crate::common::repair_watch::dispatch_or_defer_path(&target, watch_id, move || {
                    if let Some(owner) = weak.upgrade() {
                        let _gate = owner
                            .callback_gate
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        owner.refresh_admitted();
                    }
                });
            },
            Config::default(),
        )?;
        watcher.watch(root.path(), RecursiveMode::Recursive)?;
        *owner
            .watcher
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(watcher);
        owner.refresh()?;
        Ok(owner)
    }

    pub fn snapshot(&self) -> Arc<CheckedCalendarSnapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn refresh(&self) -> anyhow::Result<()> {
        let _permit = crate::common::repair_watch::require_path_read(&self.events_path)?;
        let _gate = self
            .callback_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.refresh_admitted();
        Ok(())
    }

    /// Check a displayed snapshot against the canonical file on a worker.
    pub fn snapshot_is_current(&self, snapshot: &Arc<CheckedCalendarSnapshot>) -> bool {
        let _operation = self
            .operation_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(_permit) = crate::common::repair_watch::require_path_read(&self.events_path) else {
            return false;
        };
        let _transaction = calendar_event_transaction_guard();
        if !self.active.load(Ordering::Acquire)
            || !snapshot.actionable
            || !Arc::ptr_eq(&self.snapshot(), snapshot)
        {
            return false;
        }
        let source = match std::fs::read(&self.events_path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return false,
        };
        source.as_ref() == snapshot.source.as_deref()
            && self.active.load(Ordering::Acquire)
            && Arc::ptr_eq(&self.snapshot(), snapshot)
    }

    fn watcher_error(&self, error: String) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let _transaction = calendar_event_transaction_guard();
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let previous = self.snapshot();
        let status = Some(format!("Calendar watcher failed: {error}"));
        if previous.status == status && !previous.actionable {
            return;
        }
        *self
            .snapshot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Arc::new(CheckedCalendarSnapshot {
                events: previous.events.clone(),
                revision: previous.revision.wrapping_add(1).max(1),
                status,
                actionable: false,
                source: previous.source.clone(),
            });
        (self.wake)();
    }

    fn refresh_admitted(&self) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let _transaction = calendar_event_transaction_guard();
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let previous = self.snapshot();
        let (events, status, actionable, source) = match load_events_with_bytes(&self.events_path) {
            Ok((LoadState::Missing, _)) if self.seen_loaded.load(Ordering::Acquire) => (
                previous.events.clone(),
                Some("Calendar events file is missing; retaining last-good events.".to_owned()),
                false,
                previous.source.clone(),
            ),
            Ok((LoadState::Missing, _)) => (Arc::from([]), None, true, None),
            Ok((LoadState::Empty, source)) => {
                self.seen_loaded.store(true, Ordering::Release);
                (Arc::from([]), None, true, source.map(Arc::new))
            }
            Ok((LoadState::Loaded(events), source)) => {
                self.seen_loaded.store(true, Ordering::Release);
                (Arc::from(events), None, true, source.map(Arc::new))
            }
            Err(error) => (
                previous.events.clone(),
                Some(error.to_string()),
                false,
                previous.source.clone(),
            ),
        };
        if previous.events.as_ref() == events.as_ref()
            && previous.status == status
            && previous.actionable == actionable
            && previous.source == source
        {
            return;
        }
        let next = Arc::new(CheckedCalendarSnapshot {
            events,
            revision: previous.revision.wrapping_add(1).max(1),
            status,
            actionable,
            source,
        });
        *self
            .snapshot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
        (self.wake)();
    }

    /// Update only stock Calendar state, under the shared state transaction and
    /// with exact snapshot, disk preimage and caller authority at replacement.
    pub fn write_state_if_current(
        &self,
        snapshot: &Arc<CheckedCalendarSnapshot>,
        day: NaiveDate,
        now: NaiveDateTime,
        current: impl Fn() -> bool,
    ) -> anyhow::Result<()> {
        let _operation = self
            .operation_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _events_permit = crate::common::repair_watch::require_path_read(&self.events_path)?;
        let _permit = crate::common::repair_watch::require_path_read(&self.state_path)?;
        let _transaction = CALENDAR_STATE_TRANSACTION
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let valid = || {
            self.active.load(Ordering::Acquire)
                && snapshot.actionable
                && Arc::ptr_eq(&self.snapshot(), snapshot)
                && current()
        };
        anyhow::ensure!(valid(), "Calendar request is no longer current");
        let original = match std::fs::read(&self.state_path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut state: CalendarState = match original.as_ref() {
            None => CalendarState::default(),
            Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => CalendarState::default(),
            Some(bytes) => serde_json::from_slice(bytes)?,
        };
        state.last_opened = Some(now);
        state.last_viewed_day = Some(day);
        let bytes = serde_json::to_vec_pretty(&state)?;
        let still_current = || {
            let live = match std::fs::read(&self.state_path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return false,
            };
            let events = match std::fs::read(&self.events_path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return false,
            };
            live == original && events.as_ref() == snapshot.source.as_deref() && valid()
        };
        if original.is_none() {
            // Stage on the profile volume before creating the optional Calendar
            // directory; publication must never replace a concurrent creator.
            let directory = self.state_path.parent().expect("calendar state has parent");
            let root = directory.parent().expect("calendar directory has parent");
            let mut staged = tempfile::NamedTempFile::new_in(root)?;
            staged.write_all(&bytes)?;
            staged.flush()?;
            anyhow::ensure!(still_current(), "Calendar request is no longer current");
            std::fs::create_dir_all(directory)?;
            anyhow::ensure!(still_current(), "Calendar request is no longer current");
            staged
                .persist_noclobber(&self.state_path)
                .map_err(|error| error.error)?;
        } else {
            crate::common::atomic_file::save_atomic_replaceable_if(
                &self.state_path,
                &bytes,
                still_current,
            )?;
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.active.store(false, Ordering::Release);
        let _operation = self
            .operation_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(
            self.watcher
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take(),
        );
        drop(
            self.callback_gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckedCalendarOwner, LoadState, NaiveDate, load_events_typed, relevant_event};
    use crate::platform::app_data::AppDataRoot;
    use std::sync::Arc;

    #[test]
    fn pathless_rescan_is_admitted() {
        let event =
            notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
        assert!(relevant_event(
            &event,
            std::path::Path::new("calendar/events.json"),
            std::path::Path::new("calendar")
        ));
        assert!(!relevant_event(
            &notify::Event::new(notify::EventKind::Other),
            std::path::Path::new("calendar/events.json"),
            std::path::Path::new("calendar")
        ));
    }

    #[test]
    fn shutdown_drains_manual_refresh_waiting_on_transaction_without_publication() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let owner = CheckedCalendarOwner::start(&root, Arc::new(|| {})).unwrap();
        drop(owner.watcher.lock().unwrap().take());
        let before = owner.snapshot();
        std::fs::create_dir(temp.path().join("calendar")).unwrap();
        std::fs::write(temp.path().join("calendar/events.json"), b"[]").unwrap();
        let transaction = super::calendar_event_transaction_guard();
        let refreshing = {
            let owner = owner.clone();
            std::thread::spawn(move || owner.refresh())
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while owner.callback_gate.try_lock().is_ok() {
            assert!(
                std::time::Instant::now() < deadline,
                "refresh did not acquire gate"
            );
            std::thread::yield_now();
        }
        let stopping = {
            let owner = owner.clone();
            std::thread::spawn(move || owner.shutdown())
        };
        while owner.active.load(std::sync::atomic::Ordering::Acquire) {
            assert!(
                std::time::Instant::now() < deadline,
                "shutdown did not fence owner"
            );
            std::thread::yield_now();
        }
        drop(transaction);
        refreshing.join().unwrap().unwrap();
        stopping.join().unwrap();
        assert!(Arc::ptr_eq(&before, &owner.snapshot()));
    }

    #[test]
    fn first_state_write_does_not_clobber_a_late_creator() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let owner = CheckedCalendarOwner::start(&root, Arc::new(|| {})).unwrap();
        let snapshot = owner.snapshot();
        let now = NaiveDate::from_ymd_opt(2026, 9, 6)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap();
        let calls = std::cell::Cell::new(0);
        let path = temp.path().join("calendar/state.json");
        let result = owner.write_state_if_current(&snapshot, now.date(), now, || {
            let next = calls.get() + 1;
            calls.set(next);
            if next == 3 {
                std::fs::write(&path, b"independent").unwrap();
            }
            true
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"independent");
        owner.shutdown();
    }

    #[test]
    fn watcher_observes_calendar_directory_created_after_start() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let owner = CheckedCalendarOwner::start(
            &root,
            Arc::new(move || {
                let _ = send.send(());
            }),
        )
        .unwrap();
        while receive.try_recv().is_ok() {}
        std::fs::create_dir(temp.path().join("calendar")).unwrap();
        std::fs::write(temp.path().join("calendar/events.json"), br#"[{"id":"a","title":"One","start":"2026-09-06T09:30:00","created_at":"2026-09-06T09:30:00"}]"#).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while owner.snapshot().events.is_empty() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!remaining.is_zero(), "calendar watcher did not publish");
            receive
                .recv_timeout(remaining)
                .expect("calendar watcher wake");
        }
        assert!(owner.snapshot().actionable);
        owner.shutdown();
    }

    #[test]
    fn missing_directory_is_not_created_and_invalid_store_retains_last_good() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let owner = CheckedCalendarOwner::start(&root, Arc::new(|| {})).unwrap();
        assert!(!temp.path().join("calendar").exists());
        assert!(owner.snapshot().actionable);
        std::fs::create_dir(temp.path().join("calendar")).unwrap();
        let events = temp.path().join("calendar/events.json");
        std::fs::write(&events, br#"[{"id":"a","title":"One","start":"2026-09-06T09:30:00","created_at":"2026-09-06T09:30:00"}]"#).unwrap();
        owner.refresh().unwrap();
        let good = owner.snapshot();
        assert_eq!(good.events.len(), 1);
        std::fs::write(&events, b"broken").unwrap();
        owner.refresh().unwrap();
        let invalid = owner.snapshot();
        assert_eq!(invalid.events.as_ref(), good.events.as_ref());
        assert!(!invalid.actionable);
        assert!(invalid.status.is_some());
        std::fs::remove_file(&events).unwrap();
        owner.refresh().unwrap();
        assert!(!owner.snapshot().actionable);
        owner.shutdown();
    }

    #[test]
    fn state_write_requires_current_snapshot_and_exact_disk_source() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let owner = CheckedCalendarOwner::start(&root, Arc::new(|| {})).unwrap();
        let snapshot = owner.snapshot();
        let now = NaiveDate::from_ymd_opt(2026, 9, 6)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap();
        assert!(
            owner
                .write_state_if_current(&snapshot, now.date(), now, || false)
                .is_err()
        );
        assert!(!temp.path().join("calendar").exists());
        let calls = std::cell::Cell::new(0);
        assert!(
            owner
                .write_state_if_current(&snapshot, now.date(), now, || {
                    let next = calls.get() + 1;
                    calls.set(next);
                    next == 1
                })
                .is_err()
        );
        assert!(!temp.path().join("calendar").exists());
        owner
            .write_state_if_current(&snapshot, now.date(), now, || true)
            .unwrap();
        let state_path = temp.path().join("calendar/state.json");
        let saved = std::fs::read(&state_path).unwrap();
        std::fs::write(temp.path().join("calendar/events.json"), b"[]").unwrap();
        assert!(
            owner
                .write_state_if_current(&snapshot, now.date(), now, || true)
                .is_err()
        );
        assert_eq!(std::fs::read(&state_path).unwrap(), saved);
        assert!(matches!(
            load_events_typed(temp.path().join("calendar/events.json")).unwrap(),
            LoadState::Loaded(_)
        ));
        owner.shutdown();
    }
}
