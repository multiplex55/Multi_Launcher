//! GPUI's session-scoped admission to the shared timer and Inbox owners.
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::notifications::InboxService;
use crate::platform::app_data::AppDataRoot;
use crate::plugins::timer::{self, ACTIVE_TIMERS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimerKind {
    Countdown,
    Alarm,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimerRow {
    pub id: u64,
    pub generation: u64,
    pub label: String,
    pub remaining: Duration,
    pub kind: TimerKind,
    pub paused: bool,
}

pub struct TimerSession {
    session: [u8; 16],
    alarm_path: String,
    owned: Mutex<Option<HashSet<u64>>>,
    inbox: Mutex<Option<InboxService>>,
    restore_started: AtomicBool,
}

impl TimerSession {
    pub fn start(root: AppDataRoot, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        let alarm_path = root
            .path()
            .join(timer::ALARMS_FILE)
            .to_str()
            .ok_or("Alarm path is not valid Unicode")?
            .to_owned();
        let session = rand::random::<u128>().max(1).to_le_bytes();
        if !timer::reserve_gpui_timer(session) {
            return Err("Timer notifications already have an owner".into());
        }
        let mut inbox = match InboxService::start(root, wake.clone()) {
            Ok(inbox) => inbox,
            Err(error) => {
                timer::release_gpui_timer(session);
                return Err(error.to_string());
            }
        };
        if !timer::attach_gpui_timer_inbox(inbox.publisher(), session, wake, alarm_path.clone()) {
            let _ = inbox.shutdown_checked();
            timer::release_gpui_timer(session);
            return Err("Timer notifications already have an owner".into());
        }
        Ok(Self {
            session,
            alarm_path,
            owned: Mutex::new(Some(HashSet::new())),
            inbox: Mutex::new(Some(inbox)),
            restore_started: AtomicBool::new(false),
        })
    }

    pub fn restore_alarms(&self) -> Result<(), String> {
        if self.restore_started.swap(true, Ordering::AcqRel) {
            return Err("Alarms were already restored".into());
        }
        let mut owned = self.owned.lock().map_err(|_| "Timer session unavailable")?;
        let owned = owned.as_mut().ok_or("Timer session closed")?;
        owned.extend(
            timer::restore_session_alarms(&self.alarm_path).map_err(|error| error.to_string())?,
        );
        Ok(())
    }

    pub fn start_alarm(&self, input: &str, label: &str, sound: &str) -> Result<u64, String> {
        let mut owned = self.owned.lock().map_err(|_| "Timer session unavailable")?;
        let owned = owned.as_mut().ok_or("Timer session closed")?;
        let spec = timer::parse_alarm_spec(input).map_err(|error| error.to_string())?;
        if !crate::sound::SOUND_NAMES.contains(&sound) {
            return Err("Choose a valid sound".into());
        }
        let name = (!label.is_empty()).then(|| label.to_owned());
        let id = timer::start_session_alarm(&self.alarm_path, spec, name, sound.to_owned())
            .map_err(|error| error.to_string())?;
        owned.insert(id);
        Ok(id)
    }

    pub fn start_timer(&self, duration: &str, label: &str, sound: &str) -> Result<u64, String> {
        let mut owned = self.owned.lock().map_err(|_| "Timer session unavailable")?;
        let owned = owned.as_mut().ok_or("Timer session closed")?;
        let duration =
            timer::parse_duration(duration).ok_or("Enter a positive, valid duration.")?;
        if !crate::sound::SOUND_NAMES.contains(&sound) {
            return Err("Choose a valid sound".into());
        }
        let label = if label.is_empty() {
            None
        } else {
            Some(label.to_owned())
        };
        let id = timer::try_start_timer_named(duration, label, sound.to_owned())
            .map_err(|error| error.to_string())?;
        owned.insert(id);
        Ok(id)
    }

    pub fn rows(&self) -> Vec<TimerRow> {
        self.try_rows().unwrap_or_default()
    }

    /// None means the snapshot is temporarily unavailable, not that timers ended.
    pub fn try_rows(&self) -> Option<Vec<TimerRow>> {
        let owned = self.owned.try_lock().ok()?;
        let Some(owned) = owned.as_ref() else {
            return Some(Vec::new());
        };
        let active = ACTIVE_TIMERS.try_lock().ok()?;
        let now = Instant::now();
        let mut rows: Vec<_> = owned
            .iter()
            .filter_map(|id| active.get(id))
            .map(|entry| TimerRow {
                id: entry.id,
                generation: entry.generation,
                label: entry.label.clone(),
                kind: if entry.persist { TimerKind::Alarm } else { TimerKind::Countdown },
                paused: entry.paused,
                remaining: if entry.paused {
                    entry.remaining
                } else {
                    entry.deadline.saturating_duration_since(now)
                },
            })
            .collect();
        rows.sort_by_key(|row| row.id);
        Some(rows)
    }

    pub fn cancel(&self, id: u64, generation: u64) -> Result<bool, String> {
        let Ok(mut owned) = self.owned.lock() else {
            return Err("Timer session unavailable".into());
        };
        let Some(owned) = owned.as_mut() else {
            return Err("Timer session closed".into());
        };
        if !owned.contains(&id) {
            return Ok(false);
        }
        if !timer::cancel_session_entry(&self.alarm_path, id, generation)
            .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        owned.remove(&id);
        Ok(true)
    }

    /// Toggle an exact countdown receipt while the caller's authority is live.
    pub fn set_countdown_paused_if(&self, id: u64, generation: u64, expected_paused: bool, current: impl Fn() -> bool) -> Result<bool, String> {
        let owned = self.owned.lock().map_err(|_| "Timer session unavailable")?;
        let owned = owned.as_ref().ok_or("Timer session closed")?;
        if !owned.contains(&id) { return Ok(false); }
        timer::set_session_countdown_paused_if(id, generation, expected_paused, current).map_err(|error| error.to_string())
    }

    pub fn take_finished(&self) -> Vec<String> {
        let Ok(mut owned) = self.owned.lock() else {
            return Vec::new();
        };
        let Some(owned) = owned.as_mut() else {
            return Vec::new();
        };
        let messages = timer::take_gpui_timer_messages(self.session);
        if let Ok(active) = ACTIVE_TIMERS.lock() {
            owned.retain(|id| active.contains_key(id));
        }
        messages
    }

    pub fn take_alarm_error(&self) -> Option<String> {
        timer::take_gpui_alarm_error(self.session)
    }

    /// Retire the exact timer/Inbox session; call off the UI thread.
    pub fn shutdown(&self) -> Result<(), String> {
        let mut owned = self.owned.lock().map_err(|_| "Timer session unavailable")?;
        if let Some(ids) = owned.take() {
            timer::detach_gpui_timer_inbox(self.session, &ids.into_iter().collect::<Vec<_>>());
        }
        let result = if let Some(mut inbox) = self
            .inbox
            .lock()
            .map_err(|_| "Timer Inbox unavailable")?
            .take()
        {
            inbox.shutdown_checked()
        } else {
            Ok(())
        };
        timer::release_gpui_timer(self.session);
        result
    }
}

impl Drop for TimerSession {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::TimerSession;
    use crate::platform::app_data::AppDataRoot;
    use crate::plugins::timer;
    use std::sync::Arc;

    #[test]
    fn session_start_cancel_is_generation_checked_and_retired() {
        let _owner = crate::plugins::timer::TEST_TIMER_INBOX_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let session = TimerSession::start(root, Arc::new(|| {})).unwrap();
        let second = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        assert!(TimerSession::start(second, Arc::new(|| {})).is_err());
        assert!(session.start_timer("invalid", "label", "None").is_err());
        assert!(session.rows().is_empty());
        let id = session.start_timer("1h", "exact label", "None").unwrap();
        let row = session.rows().into_iter().find(|row| row.id == id).unwrap();
        assert_eq!(row.label, "exact label");
        assert!(!session.cancel(id, row.generation.wrapping_add(1)).unwrap());
        assert!(session.cancel(id, row.generation).unwrap());
        assert!(!session.cancel(id, row.generation).unwrap());
        let still_active = session.start_timer("1h", "retire", "None").unwrap();
        session.shutdown().unwrap();
        assert!(
            !crate::plugins::timer::ACTIVE_TIMERS
                .lock()
                .unwrap()
                .contains_key(&still_active)
        );
        assert!(session.start_timer("1h", "", "None").is_err());
        let next_root = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let next = TimerSession::start(next_root.clone(), Arc::new(|| {})).unwrap();
        drop(session);
        assert!(TimerSession::start(next_root, Arc::new(|| {})).is_err());
        drop(next);
        let final_root = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let last = TimerSession::start(final_root, Arc::new(|| {})).unwrap();
        last.shutdown().unwrap();
    }

    #[test]
    fn countdown_pause_resume_rejects_stale_receipts_and_preserves_alarm_bytes() {
        use super::TimerKind;
        let _owner = crate::plugins::timer::TEST_TIMER_INBOX_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let session = TimerSession::start(root, Arc::new(|| {})).unwrap();
        session.restore_alarms().unwrap();
        let alarm = session.start_alarm("1d 12:00", "alarm", "None").unwrap();
        let alarm_bytes = std::fs::read(dir.path().join("alarms.json")).unwrap();
        let id = session.start_timer("1h", "timer", "None").unwrap();
        let original = session.rows().into_iter().find(|row| row.id == id).unwrap();
        assert_eq!(original.kind, TimerKind::Countdown);
        assert!(!original.paused);
        assert!(!session.set_countdown_paused_if(id, original.generation + 1, false, || true).unwrap());
        assert!(session.set_countdown_paused_if(id, original.generation, false, || false).is_err());
        assert!(session.set_countdown_paused_if(id, original.generation, false, || true).unwrap());
        assert_eq!(timer::session_heap_generation_for_test(id), None);
        let paused = session.rows().into_iter().find(|row| row.id == id).unwrap();
        assert!(paused.paused);
        assert_eq!(paused.generation, original.generation.wrapping_add(1));
        assert_eq!(session.rows().into_iter().find(|row| row.id == id).unwrap().remaining, paused.remaining);
        crate::plugins::timer::fire_session_timer_for_test(id, original.generation);
        assert!(session.take_finished().is_empty());
        assert!(!session.set_countdown_paused_if(id, original.generation, false, || true).unwrap());
        assert!(session.set_countdown_paused_if(id, paused.generation, true, || true).unwrap());
        assert_eq!(timer::session_heap_generation_for_test(id), Some(paused.generation.wrapping_add(1)));
        let resumed = session.rows().into_iter().find(|row| row.id == id).unwrap();
        assert!(!resumed.paused);
        assert_eq!(resumed.generation, paused.generation.wrapping_add(1));
        crate::plugins::timer::fire_session_timer_for_test(id, paused.generation);
        assert!(session.take_finished().is_empty());
        let alarm_row = session.rows().into_iter().find(|row| row.id == alarm).unwrap();
        assert_eq!(alarm_row.kind, TimerKind::Alarm);
        assert!(session.set_countdown_paused_if(alarm, alarm_row.generation, false, || true).is_err());
        assert_eq!(std::fs::read(dir.path().join("alarms.json")).unwrap(), alarm_bytes);
        session.shutdown().unwrap();
        assert!(session.set_countdown_paused_if(id, resumed.generation, false, || true).is_err());
    }

    #[test]
    fn alarm_persists_restores_and_failed_cancel_keeps_exact_bytes_and_entry() {
        let _owner = crate::plugins::timer::TEST_TIMER_INBOX_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let first = TimerSession::start(root.clone(), Arc::new(|| {})).unwrap();
        std::fs::write(dir.path().join("alarms.json"), b" \r\n").unwrap();
        first.restore_alarms().unwrap();
        let id = first.start_alarm("1d 12:00", "saved", "None").unwrap();
        let path = dir.path().join("alarms.json");
        let saved = std::fs::read(&path).unwrap();
        first.shutdown().unwrap();
        assert!(
            !crate::plugins::timer::ACTIVE_TIMERS
                .lock()
                .unwrap()
                .contains_key(&id)
        );
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        let second = TimerSession::start(root, Arc::new(|| {})).unwrap();
        second.restore_alarms().unwrap();
        assert!(second.restore_alarms().is_err());
        let row = second
            .rows()
            .into_iter()
            .find(|row| row.label == "saved")
            .unwrap();
        std::fs::write(&path, b"[]").unwrap();
        assert!(second.cancel(row.id, row.generation).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"[]");
        assert!(second.rows().iter().any(|current| current.id == row.id));
        std::fs::write(&path, b"{broken").unwrap();
        assert!(second.cancel(row.id, row.generation).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        assert!(second.rows().iter().any(|current| current.id == row.id));
        std::fs::write(&path, saved).unwrap();
        assert!(
            !second
                .cancel(row.id, row.generation.wrapping_add(1))
                .unwrap()
        );
        assert!(second.cancel(row.id, row.generation).unwrap());
        second.shutdown().unwrap();
    }
}
