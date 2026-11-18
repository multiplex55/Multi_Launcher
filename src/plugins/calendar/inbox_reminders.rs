//! Demand-driven Calendar reminder delivery. Does not alter event times.
use super::{CALENDAR_EVENTS_FILE, CalendarEvent, RecurrenceEnd, expand_instances};
use crate::{
    common::atomic_file::save_atomic,
    notifications::{
        ActionKind, ActionTicket, Category, FactId, InternalEvent, InternalFact, RelatedItem,
        Status,
    },
    platform::app_data::AppDataRoot,
};
use anyhow::{Context, Result, bail, ensure};
use chrono::{Duration as ChronoDuration, Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::File,
    io::{ErrorKind, Read},
    path::PathBuf,
    time::Duration,
};

const STORE: &str = "calendar/inbox_reminders.json";
const VERSION: u8 = 1;
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_EVENTS_FILE: u64 = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 1024;
const MAX_ITEMS: usize = 10_000;
const MAX_OFFSETS: usize = 64;
const MAX_OFFSET: i64 = 30 * 24 * 60;
const CATCHUP_DAYS: i64 = 7;
const HORIZON_DAYS: i64 = 31;
const EXPANSION_LIMIT: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Key {
    event: String,
    occurrence: i64,
    offset: i64,
    revision: u64,
    delivery: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Delivery {
    key: Key,
    due: i64,
    sent: bool,
    snoozed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u8,
    items: Vec<Delivery>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            version: VERSION,
            items: Vec::new(),
        }
    }
}

pub(crate) struct CalendarInboxReminders {
    events_path: PathBuf,
    store_path: PathBuf,
    calendar: [u8; 16],
    state: State,
}
impl CalendarInboxReminders {
    /// Loads the separate owner evidence without changing Calendar events.
    pub(crate) fn open(profile: &AppDataRoot) -> Result<Self> {
        let store_path = profile.path().join(STORE);
        let state = match File::open(&store_path) {
            Err(error) if error.kind() == ErrorKind::NotFound => State::default(),
            Err(error) => return Err(error).context("Calendar reminder evidence unavailable"),
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() as u64 <= MAX_BYTES,
                    "Calendar reminder evidence is too large"
                );
                serde_json::from_slice::<State>(&bytes)
                    .context("Calendar reminder evidence is invalid")?
            }
        };
        ensure!(
            state.version == VERSION && state.items.len() <= MAX_ITEMS,
            "Calendar reminder evidence has unsupported bounds"
        );
        let mut keys = HashSet::new();
        ensure!(
            state
                .items
                .iter()
                .all(|item| valid_key(&item.key) && keys.insert(item.key.clone())),
            "Calendar reminder evidence has invalid identities"
        );
        let calendar = calendar_identity(profile);
        ensure!(
            calendar != [0; 16],
            "Calendar profile identity is unavailable"
        );
        Ok(Self {
            events_path: profile.path().join(CALENDAR_EVENTS_FILE),
            store_path,
            calendar,
            state,
        })
    }
    /// Reads current profile events and returns the next local deadline.
    /// The delivery callback acknowledges the Inbox transaction, not queue admission.
    pub(crate) fn tick(
        &mut self,
        now: NaiveDateTime,
        mut publish: impl FnMut(InternalEvent) -> bool,
    ) -> Result<Option<NaiveDateTime>> {
        let events = self.read_events()?;
        let before = now
            .checked_sub_signed(ChronoDuration::days(CATCHUP_DAYS))
            .context("Calendar time out of range")?;
        let horizon = now
            .checked_add_signed(ChronoDuration::days(HORIZON_DAYS))
            .context("Calendar time out of range")?;
        let end = horizon
            .checked_add_signed(ChronoDuration::minutes(MAX_OFFSET))
            .context("Calendar time out of range")?;
        let mut candidate = self.state.clone();
        let prune_before = now
            .checked_sub_signed(ChronoDuration::days(90))
            .context("Calendar time out of range")?
            .and_utc()
            .timestamp();
        let old_count = candidate.items.len();
        let catchup_start = before.and_utc().timestamp();
        let mut retained = Vec::with_capacity(old_count);
        for item in candidate.items {
            let retain = if item.sent {
                item.due >= prune_before
            } else {
                item.due >= catchup_start && current_key(&events, &item.key)?
            };
            if retain {
                retained.push(item);
            }
        }
        candidate.items = retained;
        let mut changed = candidate.items.len() != old_count;
        let mut next: Option<NaiveDateTime> = None;
        let mut known: HashSet<Key> = candidate
            .items
            .iter()
            .map(|item| item.key.clone())
            .collect();
        let mut future_configured = false;
        for event in &events {
            if event.reminders.is_empty() {
                continue;
            }
            future_configured |= event.recurrence.is_some() || event.start > now;
            let revision = revision(event)?;
            let instances = expand_checked(event, before, end)?;
            for instance in instances {
                for reminder in &event.reminders {
                    let due = instance
                        .start
                        .checked_sub_signed(ChronoDuration::minutes(reminder.minutes_before))
                        .context("Calendar reminder time out of range")?;
                    if due < before || due > horizon {
                        continue;
                    }
                    let key = Key {
                        event: event.id.clone(),
                        occurrence: instance.start.and_utc().timestamp(),
                        offset: reminder.minutes_before,
                        revision,
                        delivery: 0,
                    };
                    if due > now {
                        if !known.contains(&key) {
                            next = Some(next.map_or(due, |existing| existing.min(due)));
                        }
                        continue;
                    }
                    if known.insert(key.clone()) {
                        ensure!(
                            candidate.items.len() < MAX_ITEMS,
                            "Calendar reminder evidence reached its limit"
                        );
                        changed = true;
                        candidate.items.push(Delivery {
                            key,
                            due: due.and_utc().timestamp(),
                            sent: false,
                            snoozed: false,
                        });
                    }
                }
            }
        }
        if changed {
            self.commit(candidate)?;
        }
        if future_configured && next.is_none() {
            next = Some(horizon);
        }
        for index in 0..self.state.items.len() {
            let item = &self.state.items[index];
            if item.sent {
                continue;
            }
            let Some(due) =
                chrono::DateTime::from_timestamp(item.due, 0).map(|time| time.naive_utc())
            else {
                bail!("Calendar reminder due time is invalid")
            };
            if !current_key(&events, &item.key)? {
                continue;
            }
            if due > now {
                next = Some(next.map_or(due, |existing| existing.min(due)));
                continue;
            }
            let instant = Local
                .from_local_datetime(&due)
                .earliest()
                .or_else(|| Local.from_local_datetime(&now).earliest())
                .context("Calendar reminder has no valid local time")?;
            let event = fact(self.calendar, &item.key, instant.timestamp());
            if !publish(event) {
                return Ok(Some(
                    now.checked_add_signed(ChronoDuration::seconds(1))
                        .context("Calendar time out of range")?,
                ));
            }
            let mut updated = self.state.clone();
            updated.items[index].sent = true;
            self.commit(updated)?;
        }
        Ok(next)
    }
    /// Revalidates current Calendar data and records one local deferral for the
    /// exact delivered ticket. The caller also checks the Inbox action ticket.
    pub(crate) fn snooze(
        &mut self,
        ticket: &ActionTicket,
        duration: Duration,
        now: NaiveDateTime,
    ) -> Result<()> {
        ensure!(
            ticket.kind == ActionKind::Snooze,
            "Calendar action is not a snooze"
        );
        ensure!(
            [300, 600, 900].contains(&duration.as_secs()) && duration.subsec_nanos() == 0,
            "Unsupported snooze duration"
        );
        let FactId::Calendar {
            calendar,
            event,
            occurrence,
            reminder_offset,
            revision,
            delivery,
        } = &ticket.id
        else {
            bail!("Calendar action does not name a reminder")
        };
        ensure!(*calendar == self.calendar, "Calendar profile changed");
        let key = Key {
            event: event.clone(),
            occurrence: *occurrence,
            offset: *reminder_offset,
            revision: *revision,
            delivery: *delivery,
        };
        ensure!(
            current_key(&self.read_events()?, &key)?,
            "Calendar reminder changed"
        );
        let index = self
            .state
            .items
            .iter()
            .position(|item| item.key == key)
            .context("Calendar reminder no longer exists")?;
        ensure!(
            self.state.items[index].sent && !self.state.items[index].snoozed,
            "Calendar reminder was already snoozed or not delivered"
        );
        let delivery = key
            .delivery
            .checked_add(1)
            .context("Calendar reminder delivery limit reached")?;
        let due = now
            .checked_add_signed(ChronoDuration::seconds(duration.as_secs() as i64))
            .context("Calendar snooze time out of range")?;
        let mut candidate = self.state.clone();
        ensure!(
            candidate.items.len() < MAX_ITEMS,
            "Calendar reminder evidence reached its limit"
        );
        candidate.items[index].snoozed = true;
        let next = Key { delivery, ..key };
        ensure!(
            !candidate.items.iter().any(|item| item.key == next),
            "Calendar reminder was already snoozed"
        );
        candidate.items.push(Delivery {
            key: next,
            due: due.and_utc().timestamp(),
            sent: false,
            snoozed: false,
        });
        self.commit(candidate)
    }
    fn read_events(&self) -> Result<Vec<CalendarEvent>> {
        let file = match File::open(&self.events_path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error).context("Calendar events unavailable"),
        };
        let mut bytes = Vec::new();
        file.take(MAX_EVENTS_FILE + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_EVENTS_FILE,
            "Calendar events exceed reminder limits"
        );
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(Vec::new());
        }
        let events: Vec<CalendarEvent> =
            serde_json::from_slice(&bytes).context("Calendar events are malformed")?;
        ensure!(
            events.len() <= MAX_EVENTS,
            "Too many Calendar events for reminders"
        );
        let mut ids = HashSet::new();
        for event in &events {
            ensure!(
                !event.id.is_empty()
                    && event.id.len() <= 256
                    && !event.id.chars().any(char::is_control)
                    && ids.insert(&event.id),
                "Calendar event identity is invalid or duplicated"
            );
            ensure!(
                event.reminders.len() <= MAX_OFFSETS
                    && event
                        .reminders
                        .iter()
                        .all(|r| (0..=MAX_OFFSET).contains(&r.minutes_before)),
                "Calendar reminder offsets exceed limits"
            );
        }
        Ok(events)
    }
    fn commit(&mut self, candidate: State) -> Result<()> {
        let bytes = serde_json::to_vec(&candidate)?;
        ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Calendar reminder evidence is too large"
        );
        save_atomic(&self.store_path, &bytes).context("Save Calendar reminder evidence")?;
        self.state = candidate;
        Ok(())
    }
}
fn valid_key(key: &Key) -> bool {
    !key.event.is_empty()
        && key.event.len() <= 256
        && !key.event.chars().any(char::is_control)
        && (0..=MAX_OFFSET).contains(&key.offset)
}
fn revision(event: &CalendarEvent) -> Result<u64> {
    // Exclude private title/notes; include only timing and reminder semantics.
    let bytes = serde_json::to_vec(&(
        event.start.and_utc().timestamp(),
        event.end.map(|time| time.and_utc().timestamp()),
        event.duration_minutes,
        event.all_day,
        &event.recurrence,
        &event.reminders,
    ))?;
    let hash = Sha256::digest(bytes);
    Ok(u64::from_le_bytes(hash[..8].try_into().unwrap()))
}
fn expand_checked(
    event: &CalendarEvent,
    start: NaiveDateTime,
    end: NaiveDateTime,
) -> Result<Vec<super::EventInstance>> {
    if event
        .recurrence
        .as_ref()
        .is_some_and(|rule| matches!(&rule.end, RecurrenceEnd::Never))
        && start
            .checked_sub_signed(ChronoDuration::days(40 * 365))
            .is_some_and(|oldest| event.start < oldest)
    {
        bail!("Recurring reminder exceeds bounded lookback");
    }
    let rows = std::panic::catch_unwind(|| {
        expand_instances(std::slice::from_ref(event), start, end, EXPANSION_LIMIT)
    })
    .map_err(|_| anyhow::anyhow!("Calendar recurrence could not be expanded"))?;
    ensure!(
        rows.len() < EXPANSION_LIMIT,
        "Calendar recurrence exceeds reminder limits"
    );
    Ok(rows)
}
fn calendar_identity(profile: &AppDataRoot) -> [u8; 16] {
    let digest = Sha256::digest(profile.normalized_identity().as_bytes());
    let mut calendar = [0; 16];
    calendar.copy_from_slice(&digest[..16]);
    calendar
}

pub(crate) fn related_inbox_calendar_occurrence(
    profile: &AppDataRoot,
    id: &FactId,
) -> Option<super::EventInstance> {
    let FactId::Calendar {
        calendar,
        event,
        occurrence,
        reminder_offset,
        revision,
        delivery,
    } = id
    else {
        return None;
    };
    if *calendar != calendar_identity(profile) {
        return None;
    }
    let key = Key {
        event: event.clone(),
        occurrence: *occurrence,
        offset: *reminder_offset,
        revision: *revision,
        delivery: *delivery,
    };
    let events = super::CALENDAR_DATA.read().ok()?;
    if !current_key(&events, &key).ok()? {
        return None;
    }
    let event = events.iter().find(|entry| entry.id == key.event)?;
    let occurrence = chrono::DateTime::from_timestamp(key.occurrence, 0)?.naive_utc();
    let end = occurrence.checked_add_signed(ChronoDuration::seconds(1))?;
    expand_checked(event, occurrence, end)
        .ok()?
        .into_iter()
        .find(|instance| instance.start == occurrence)
}

fn current_key(events: &[CalendarEvent], key: &Key) -> Result<bool> {
    let Some(event) = events.iter().find(|event| event.id == key.event) else {
        return Ok(false);
    };
    if revision(event)? != key.revision
        || !event
            .reminders
            .iter()
            .any(|r| r.minutes_before == key.offset)
    {
        return Ok(false);
    }
    let Some(occurrence) =
        chrono::DateTime::from_timestamp(key.occurrence, 0).map(|time| time.naive_utc())
    else {
        return Ok(false);
    };
    let Some(end) = occurrence.checked_add_signed(ChronoDuration::seconds(1)) else {
        return Ok(false);
    };
    Ok(expand_checked(event, occurrence, end)?
        .iter()
        .any(|instance| instance.start == occurrence))
}
fn fact(calendar: [u8; 16], key: &Key, timestamp: i64) -> InternalEvent {
    InternalEvent {
        id: FactId::Calendar {
            calendar,
            event: key.event.clone(),
            occurrence: key.occurrence,
            reminder_offset: key.offset,
            revision: key.revision,
            delivery: key.delivery,
        },
        occurred_at: timestamp,
        category: Category::Reminder,
        status: Status::Current,
        fact: InternalFact::CalendarDue,
        related: Some(RelatedItem::Calendar {
            calendar,
            event: key.event.clone(),
            occurrence: key.occurrence,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::{Inbox, Ingest};
    fn setup() -> (tempfile::TempDir, AppDataRoot, CalendarEvent, NaiveDateTime) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("calendar")).unwrap();
        let profile = AppDataRoot::from_path(dir.path());
        let now = chrono::Local::now()
            .naive_local()
            .with_nanosecond(0)
            .unwrap();
        let event: CalendarEvent = serde_json::from_value(serde_json::json!({"id":"evt-fixture", "title":"private", "start":(now + ChronoDuration::minutes(5)).format("%Y-%m-%dT%H:%M:%S").to_string(), "reminders":[{"minutes_before":5}]})).unwrap();
        std::fs::write(
            dir.path().join(CALENDAR_EVENTS_FILE),
            serde_json::to_vec(&vec![event.clone()]).unwrap(),
        )
        .unwrap();
        (dir, profile, event, now)
    }
    use chrono::Timelike;
    #[test]
    fn notification_inbox_calendar_waits_without_writing_and_caps_the_opened_file() {
        let (dir, profile, _, due) = setup();
        let mut owner = CalendarInboxReminders::open(&profile).unwrap();
        let future = due - ChronoDuration::minutes(10);
        let next = owner
            .tick(future, |_| panic!("Future reminder is not due."))
            .unwrap();
        assert_eq!(next, Some(due));
        assert!(!dir.path().join(STORE).exists());

        let path = dir.path().join(CALENDAR_EVENTS_FILE);
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_len(MAX_EVENTS_FILE + 1)
            .unwrap();
        assert!(
            owner
                .tick(due, |_| panic!("Oversized profile cannot deliver."))
                .is_err()
        );
        assert!(!dir.path().join(STORE).exists());
    }

    #[test]
    fn notification_inbox_calendar_due_once_and_snooze_without_event_mutation() {
        let (dir, profile, _, now) = setup();
        let path = dir.path().join(CALENDAR_EVENTS_FILE);
        let original = std::fs::read(&path).unwrap();
        let mut owner = CalendarInboxReminders::open(&profile).unwrap();
        let mut emitted = Vec::new();
        owner
            .tick(now, |event| {
                emitted.push(event);
                true
            })
            .unwrap();
        assert_eq!(emitted.len(), 1);
        owner
            .tick(now, |event| {
                emitted.push(event);
                true
            })
            .unwrap();
        assert_eq!(emitted.len(), 1);
        let first = &emitted[0];
        let mut inbox = Inbox::default();
        assert!(matches!(
            inbox.ingest_internal(
                first.id.clone(),
                first.occurred_at,
                first.category,
                first.status,
                first.fact,
                first.related.clone(),
                first.occurred_at
            ),
            Ingest::Added { .. }
        ));
        let ticket = inbox.begin_action(&first.id, ActionKind::Snooze).unwrap();
        owner
            .snooze(&ticket, Duration::from_secs(300), now)
            .unwrap();
        assert!(
            owner
                .snooze(&ticket, Duration::from_secs(300), now)
                .is_err()
        );
        owner
            .tick(now + ChronoDuration::minutes(5), |event| {
                emitted.push(event);
                true
            })
            .unwrap();
        assert_eq!(emitted.len(), 2);
        assert_ne!(emitted[0].id, emitted[1].id);
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
    #[test]
    fn notification_inbox_calendar_stale_snooze_and_rejected_publisher() {
        let (dir, profile, mut event, now) = setup();
        let mut owner = CalendarInboxReminders::open(&profile).unwrap();
        let mut first = None;
        assert!(owner.tick(now, |_| false).unwrap().is_some());
        owner
            .tick(now, |event| {
                first = Some(event);
                true
            })
            .unwrap();
        let first = first.unwrap();
        let mut inbox = Inbox::default();
        inbox.ingest_internal(
            first.id.clone(),
            first.occurred_at,
            first.category,
            first.status,
            first.fact,
            first.related,
            first.occurred_at,
        );
        let ticket = inbox.begin_action(&first.id, ActionKind::Snooze).unwrap();
        event.start += ChronoDuration::minutes(1);
        std::fs::write(
            dir.path().join(CALENDAR_EVENTS_FILE),
            serde_json::to_vec(&vec![event]).unwrap(),
        )
        .unwrap();
        assert!(
            owner
                .snooze(&ticket, Duration::from_secs(300), now)
                .is_err()
        );
    }
    #[test]
    fn notification_inbox_calendar_preserves_malformed_evidence() {
        let (dir, profile, _, _) = setup();
        let path = dir.path().join(STORE);
        std::fs::write(&path, b"bad").unwrap();
        assert!(CalendarInboxReminders::open(&profile).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"bad");
    }
}
