use crate::actions::Action;
use crate::common::persistence::{LoadState, load_json, save_json_atomic_replaceable};
use crate::plugin::Plugin;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub static ALARMS_LOADED: AtomicBool = AtomicBool::new(false);
pub const ALARMS_FILE: &str = "alarms.json";
pub static FINISHED_MESSAGES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub struct TimerEntry {
    pub id: u64,
    pub label: String,
    pub deadline: Instant,
    pub persist: bool,
    pub end_ts: u64,
    pub start_ts: u64,
    pub paused: bool,
    pub remaining: Duration,
    pub generation: u64,
    pub sound: String,
}

#[derive(Serialize, Deserialize)]
struct SavedAlarm {
    label: String,
    end_ts: u64,
    #[serde(default)]
    start_ts: u64,
    #[serde(default)]
    sound: String,
}

fn save_persistent_alarms_locked(timers: &HashMap<u64, TimerEntry>) {
    if let Err(error) = save_persistent_alarms_to(ALARMS_FILE, timers) {
        tracing::error!(%error, "alarm persistence skipped");
    }
}

fn save_persistent_alarms_to(path: &str, timers: &HashMap<u64, TimerEntry>) -> anyhow::Result<()> {
    save_persistent_alarms_with_candidate(path, timers, None)
}

fn save_persistent_alarms_with_candidate(
    path: &str,
    timers: &HashMap<u64, TimerEntry>,
    candidate: Option<&TimerEntry>,
) -> anyhow::Result<()> {
    save_persistent_alarms_with_candidate_using(path, timers, candidate, |path, list| {
        save_json_atomic_replaceable(path, list)?;
        Ok(())
    })
}

fn save_persistent_alarms_with_candidate_using(
    path: &str,
    timers: &HashMap<u64, TimerEntry>,
    candidate: Option<&TimerEntry>,
    write: impl FnOnce(&str, &[SavedAlarm]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut list: Vec<SavedAlarm> = timers
        .values()
        .filter(|timer| timer.persist)
        .map(saved_alarm)
        .collect();
    if let Some(candidate) = candidate.filter(|timer| timer.persist) {
        list.push(saved_alarm(candidate));
    }
    match load_json::<Vec<SavedAlarm>>(path) {
        Ok(LoadState::Missing | LoadState::Empty | LoadState::Loaded(_)) => write(path, &list),
        Err(error) => Err(error.into()),
    }
}

fn saved_alarm(timer: &TimerEntry) -> SavedAlarm {
    SavedAlarm {
        label: timer.label.clone(),
        end_ts: timer.end_ts,
        start_ts: timer.start_ts,
        sound: timer.sound.clone(),
    }
}

pub static ACTIVE_TIMERS: Lazy<Mutex<HashMap<u64, TimerEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerMutation {
    Updated,
    Missing,
    AlreadyPaused,
    AlreadyRunning,
}

struct TimerManager {
    inner: Arc<Inner>,
}

struct HeapEntry {
    deadline: Instant,
    id: u64,
    generation: u64,
}

struct TimerHeap {
    entries: Vec<HeapEntry>,
    positions: HashMap<u64, usize>,
}

impl TimerHeap {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            positions: HashMap::new(),
        }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn peek(&self) -> Option<&HeapEntry> {
        self.entries.first()
    }

    fn push(&mut self, entry: HeapEntry) {
        let idx = self.entries.len();
        self.entries.push(entry);
        self.positions.insert(self.entries[idx].id, idx);
        self.sift_up(idx);
    }

    fn remove(&mut self, id: u64) {
        if let Some(&idx) = self.positions.get(&id) {
            self.remove_at(idx);
        }
    }

    fn pop(&mut self) -> Option<HeapEntry> {
        if self.entries.is_empty() {
            return None;
        }
        let last = self.entries.len() - 1;
        self.entries.swap(0, last);
        let entry = self.entries.pop().unwrap();
        self.positions.remove(&entry.id);
        if !self.entries.is_empty() {
            self.positions.insert(self.entries[0].id, 0);
            self.sift_down(0);
        }
        Some(entry)
    }

    fn remove_at(&mut self, idx: usize) {
        let last = self.entries.len() - 1;
        if idx != last {
            self.entries.swap(idx, last);
            let id = self.entries[idx].id;
            self.positions.insert(id, idx);
        }
        let removed = self.entries.pop().unwrap();
        self.positions.remove(&removed.id);
        if idx < self.entries.len() && !self.sift_down(idx) {
            self.sift_up(idx);
        }
    }

    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.entries[idx].deadline < self.entries[parent].deadline {
                self.entries.swap(idx, parent);
                let id = self.entries[idx].id;
                let pid = self.entries[parent].id;
                self.positions.insert(id, idx);
                self.positions.insert(pid, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, mut idx: usize) -> bool {
        let len = self.entries.len();
        let mut moved = false;
        loop {
            let left = idx * 2 + 1;
            let right = left + 1;
            let mut smallest = idx;
            if left < len && self.entries[left].deadline < self.entries[smallest].deadline {
                smallest = left;
            }
            if right < len && self.entries[right].deadline < self.entries[smallest].deadline {
                smallest = right;
            }
            if smallest != idx {
                self.entries.swap(idx, smallest);
                let id = self.entries[idx].id;
                let sid = self.entries[smallest].id;
                self.positions.insert(id, idx);
                self.positions.insert(sid, smallest);
                idx = smallest;
                moved = true;
            } else {
                break;
            }
        }
        moved
    }
}

struct Inner {
    heap: Mutex<TimerHeap>,
    condvar: Condvar,
}

impl TimerManager {
    fn new() -> Self {
        let inner = Arc::new(Inner {
            heap: Mutex::new(TimerHeap::new()),
            condvar: Condvar::new(),
        });
        let thread_inner = inner.clone();
        thread::spawn(move || Self::run(thread_inner));
        Self { inner }
    }

    fn run(inner: Arc<Inner>) {
        let mut heap_guard = match inner.heap.lock().ok() {
            Some(g) => g,
            None => return,
        };
        loop {
            while heap_guard.peek().is_none() {
                heap_guard = match inner.condvar.wait(heap_guard).ok() {
                    Some(g) => g,
                    None => return,
                };
            }
            let now = Instant::now();
            if let Some(entry) = heap_guard.peek() {
                if entry.deadline <= now {
                    let popped = heap_guard.pop().unwrap();
                    drop(heap_guard);
                    Self::fire_timer(popped.id, popped.generation);
                    heap_guard = match inner.heap.lock().ok() {
                        Some(g) => g,
                        None => return,
                    };
                    continue;
                } else {
                    let wait = entry.deadline.saturating_duration_since(now);
                    let res = match inner.condvar.wait_timeout(heap_guard, wait).ok() {
                        Some(r) => r,
                        None => return,
                    };
                    heap_guard = res.0;
                    continue;
                }
            }
        }
    }

    fn fire_timer(id: u64, generation: u64) {
        let mut list = match ACTIVE_TIMERS.lock().ok() {
            Some(l) => l,
            None => return,
        };
        if let Some(entry) = list.get(&id)
            && entry.generation == generation
            && !entry.paused
            && let Some(entry) = list.remove(&id)
        {
            if entry.persist {
                save_persistent_alarms_locked(&list);
            }
            drop(list);
            let msg = if entry.persist {
                format!("Alarm triggered: {}", entry.label)
            } else {
                format!("Timer finished: {}", entry.label)
            };
            crate::sound::play_sound(&entry.sound);
            notify(&msg);
            if let Ok(mut msgs) = FINISHED_MESSAGES.lock() {
                msgs.push(msg);
            }
        }
    }

    fn register(&self, deadline: Instant, id: u64, generation: u64) {
        if let Ok(mut heap) = self.inner.heap.lock() {
            heap.remove(id);
            heap.push(HeapEntry {
                deadline,
                id,
                generation,
            });
            self.inner.condvar.notify_one();
        }
    }

    fn wakeup(&self) {
        self.inner.condvar.notify_one();
    }

    fn remove(&self, id: u64) {
        if let Ok(mut heap) = self.inner.heap.lock() {
            heap.remove(id);
            self.inner.condvar.notify_one();
        }
    }
}

static TIMER_MANAGER: Lazy<TimerManager> = Lazy::new(TimerManager::new);

pub fn heap_len() -> usize {
    TIMER_MANAGER
        .inner
        .heap
        .lock()
        .map(|h| h.len())
        .unwrap_or(0)
}

/// Reset the flag tracking whether saved alarms have been loaded.
pub fn reset_alarms_loaded() {
    ALARMS_LOADED.store(false, Ordering::SeqCst);
}

/// Retrieve and clear finished timer/alarm notifications.
pub fn take_finished_messages() -> Vec<String> {
    if let Ok(mut list) = FINISHED_MESSAGES.lock() {
        let out = list.clone();
        list.clear();
        out
    } else {
        Vec::new()
    }
}

/// Load persisted alarms from the alarms file if not already loaded.
pub fn load_saved_alarms() {
    if ALARMS_LOADED.load(Ordering::SeqCst) {
        return;
    }
    use chrono::Local;
    let wall_now = Local::now();
    let monotonic_now = Instant::now();
    let entries = match load_saved_alarm_entries(ALARMS_FILE, wall_now, monotonic_now) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::error!(%error, "failed to load alarms; retaining the existing file");
            ALARMS_LOADED.store(true, Ordering::SeqCst);
            return;
        }
    };
    let mut restored_any = false;
    for entry in entries {
        match publish_restored_entry(entry) {
            Ok(_) => restored_any = true,
            Err(error) => tracing::error!(%error, "failed to activate a restored alarm"),
        }
    }
    if restored_any && let Ok(timers) = ACTIVE_TIMERS.lock() {
        save_persistent_alarms_locked(&timers);
    }
    ALARMS_LOADED.store(true, Ordering::SeqCst);
}

fn load_saved_alarm_entries(
    path: &str,
    wall_now: chrono::DateTime<chrono::Local>,
    monotonic_now: Instant,
) -> anyhow::Result<Vec<TimerEntry>> {
    use chrono::Utc;

    let alarms = match load_json::<Vec<SavedAlarm>>(path)? {
        LoadState::Missing | LoadState::Empty => return Ok(Vec::new()),
        LoadState::Loaded(alarms) => alarms,
    };
    let utc_now = wall_now.with_timezone(&Utc);
    let current_start_ts = unix_timestamp(wall_now.timestamp())?;
    let mut entries = Vec::with_capacity(alarms.len());
    for alarm in alarms {
        let Some(remaining) = remaining_until_timestamp(alarm.end_ts, &utc_now) else {
            continue;
        };
        let Some(deadline) = monotonic_now.checked_add(remaining) else {
            tracing::warn!("skipping alarm whose deadline exceeds the monotonic clock range");
            continue;
        };
        let Ok(id) = allocate_timer_id() else {
            tracing::error!("unable to allocate an id for a restored alarm");
            continue;
        };
        entries.push(TimerEntry {
            id,
            label: alarm.label,
            deadline,
            persist: true,
            end_ts: alarm.end_ts,
            start_ts: if alarm.start_ts > 0 {
                alarm.start_ts
            } else {
                current_start_ts
            },
            paused: false,
            remaining,
            generation: 0,
            sound: alarm.sound,
        });
    }
    Ok(entries)
}

fn remaining_until_timestamp(
    timestamp: u64,
    now: &chrono::DateTime<chrono::Utc>,
) -> Option<Duration> {
    use chrono::TimeZone;
    let timestamp = i64::try_from(timestamp).ok()?;
    let target = chrono::Utc.timestamp_opt(timestamp, 0).single()?;
    let remaining = target.signed_duration_since(*now).to_std().ok()?;
    (!remaining.is_zero()).then_some(remaining)
}

/// Parse a positive duration string.
///
/// Supports `Ns`, `Nm`, `Nh` as well as `hh:mm:ss` and `mm:ss` formats.
pub fn parse_duration(input: &str) -> Option<Duration> {
    let input = input.trim();
    if input.contains(':') {
        let parts: Vec<&str> = input.split(':').collect();
        if parts.len() == 2 || parts.len() == 3 {
            let (hours, minutes, seconds) = if parts.len() == 3 {
                (
                    parts[0].parse::<u64>().ok()?,
                    parts[1].parse::<u64>().ok()?,
                    parts[2].parse::<u64>().ok()?,
                )
            } else {
                (
                    0,
                    parts[0].parse::<u64>().ok()?,
                    parts[1].parse::<u64>().ok()?,
                )
            };
            if minutes >= 60 || seconds >= 60 {
                return None;
            }
            let total = hours
                .checked_mul(3_600)?
                .checked_add(minutes.checked_mul(60)?)?
                .checked_add(seconds)?;
            return (total > 0).then(|| Duration::from_secs(total));
        }
        return None;
    }
    let (unit_index, unit) = input.char_indices().last()?;
    let value: u64 = input.get(..unit_index)?.parse().ok()?;
    let seconds = match unit.to_ascii_lowercase() {
        's' => value,
        'm' => value.checked_mul(60)?,
        'h' => value.checked_mul(3_600)?,
        _ => return None,
    };
    (seconds > 0).then(|| Duration::from_secs(seconds))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlarmDateSpec {
    NextOccurrence,
    DayOffset(i64),
    ExplicitDate(chrono::NaiveDate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlarmSpec {
    pub hour: u32,
    pub minute: u32,
    pub date: AlarmDateSpec,
}

/// Parse `HH:MM`, `Nd HH:MM`, or `YYYY-MM-DD HH:MM` without consulting a clock.
pub fn parse_alarm_spec(input: &str) -> anyhow::Result<AlarmSpec> {
    use chrono::NaiveDate;

    fn parse_time(input: &str) -> anyhow::Result<(u32, u32)> {
        let mut parts = input.split(':');
        let hour = parts
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|hour| *hour < 24)
            .ok_or_else(|| anyhow::anyhow!("invalid alarm time"))?;
        let minute = parts
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|minute| *minute < 60)
            .ok_or_else(|| anyhow::anyhow!("invalid alarm time"))?;
        if parts.next().is_some() {
            anyhow::bail!("invalid alarm time");
        }
        Ok((hour, minute))
    }

    let mut parts = input.split_whitespace();
    let first = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid alarm time"))?;
    let second = parts.next();
    if parts.next().is_some() {
        anyhow::bail!("invalid alarm time");
    }

    let (hour, minute, date) = match second {
        None => {
            let (hour, minute) = parse_time(first)?;
            (hour, minute, AlarmDateSpec::NextOccurrence)
        }
        Some(time) => {
            let (hour, minute) = parse_time(time)?;
            let date = if let Some(days) = first.strip_suffix(['d', 'D']) {
                AlarmDateSpec::DayOffset(
                    days.parse::<i64>()
                        .map_err(|_| anyhow::anyhow!("invalid alarm date offset"))?,
                )
            } else {
                AlarmDateSpec::ExplicitDate(
                    NaiveDate::parse_from_str(first, "%Y-%m-%d")
                        .map_err(|_| anyhow::anyhow!("invalid alarm date"))?,
                )
            };
            (hour, minute, date)
        }
    };

    Ok(AlarmSpec { hour, minute, date })
}

fn alarm_spec_legacy_parts(
    spec: AlarmSpec,
    today: Option<chrono::NaiveDate>,
) -> Option<(u32, u32, Option<chrono::NaiveDate>)> {
    let date = match spec.date {
        AlarmDateSpec::NextOccurrence => None,
        AlarmDateSpec::DayOffset(offset) => Some(shift_date(today?, offset)?),
        AlarmDateSpec::ExplicitDate(date) => Some(date),
    };
    Some((spec.hour, spec.minute, date))
}

/// Compatibility adapter for the legacy `(hour, minute, date)` parser contract.
///
/// New scheduling code must use [`parse_alarm_spec`] and the checked timezone resolver. Only the
/// legacy day-offset conversion consults the current local date.
pub fn parse_hhmm(input: &str) -> Option<(u32, u32, Option<chrono::NaiveDate>)> {
    let spec = parse_alarm_spec(input).ok()?;
    let today =
        matches!(spec.date, AlarmDateSpec::DayOffset(_)).then(|| chrono::Local::now().date_naive());
    alarm_spec_legacy_parts(spec, today)
}

/// Search up to one calendar year for the next valid local wall-clock occurrence.
pub const MAX_NEXT_OCCURRENCE_DAYS: u32 = 366;

/// Resolve an alarm through a caller-supplied timezone mapping, which makes DST behavior testable.
pub fn resolve_alarm_spec_at<Tz, Map>(
    spec: AlarmSpec,
    now: chrono::DateTime<Tz>,
    mut map_local: Map,
) -> anyhow::Result<chrono::DateTime<Tz>>
where
    Tz: chrono::TimeZone,
    Map: FnMut(chrono::NaiveDateTime) -> chrono::LocalResult<chrono::DateTime<Tz>>,
{
    let first_date = match spec.date {
        AlarmDateSpec::NextOccurrence => now.date_naive(),
        AlarmDateSpec::DayOffset(offset) => shift_date(now.date_naive(), offset)
            .ok_or_else(|| anyhow::anyhow!("alarm date is outside the supported range"))?,
        AlarmDateSpec::ExplicitDate(date) => date,
    };

    if matches!(spec.date, AlarmDateSpec::NextOccurrence) {
        for offset in 0..MAX_NEXT_OCCURRENCE_DAYS {
            let Some(date) = first_date.checked_add_days(chrono::Days::new(u64::from(offset)))
            else {
                break;
            };
            if let Some(target) = resolve_alarm_on_date(spec, date, &now, &mut map_local) {
                return Ok(target);
            }
        }
        anyhow::bail!("no valid future alarm time found within one calendar year");
    }

    match resolve_alarm_on_date(spec, first_date, &now, &mut map_local) {
        Some(target) => Ok(target),
        None => anyhow::bail!("alarm time must be a valid future local time"),
    }
}

fn shift_date(date: chrono::NaiveDate, offset: i64) -> Option<chrono::NaiveDate> {
    if offset >= 0 {
        date.checked_add_days(chrono::Days::new(offset as u64))
    } else {
        date.checked_sub_days(chrono::Days::new(offset.unsigned_abs()))
    }
}

fn resolve_alarm_on_date<Tz, Map>(
    spec: AlarmSpec,
    date: chrono::NaiveDate,
    now: &chrono::DateTime<Tz>,
    map_local: &mut Map,
) -> Option<chrono::DateTime<Tz>>
where
    Tz: chrono::TimeZone,
    Map: FnMut(chrono::NaiveDateTime) -> chrono::LocalResult<chrono::DateTime<Tz>>,
{
    let local = date.and_hms_opt(spec.hour, spec.minute, 0)?;
    match map_local(local) {
        chrono::LocalResult::Single(candidate) => {
            is_strictly_future(&candidate, now).then_some(candidate)
        }
        chrono::LocalResult::Ambiguous(first, second) => {
            let (earlier, later) = if instant_key(&first) <= instant_key(&second) {
                (first, second)
            } else {
                (second, first)
            };
            if is_strictly_future(&earlier, now) {
                Some(earlier)
            } else if is_strictly_future(&later, now) {
                Some(later)
            } else {
                None
            }
        }
        chrono::LocalResult::None => None,
    }
}

fn instant_key<Tz: chrono::TimeZone>(value: &chrono::DateTime<Tz>) -> (i64, u32) {
    (value.timestamp(), value.timestamp_subsec_nanos())
}

fn is_strictly_future<Tz: chrono::TimeZone>(
    candidate: &chrono::DateTime<Tz>,
    now: &chrono::DateTime<Tz>,
) -> bool {
    instant_key(candidate) > instant_key(now)
}

/// Return a list of active timers with remaining time.
pub fn active_timers() -> Vec<(u64, String, Duration, u64)> {
    let now = Instant::now();
    let Some(timers) = ACTIVE_TIMERS.lock().ok() else {
        return Vec::new();
    };
    timers
        .values()
        .map(|t| {
            let remaining = if t.paused {
                t.remaining
            } else {
                t.deadline.saturating_duration_since(now)
            };
            (t.id, t.label.clone(), remaining, t.start_ts)
        })
        .collect()
}

/// Return active timers that are currently running (not paused).
pub fn running_timers() -> Vec<(u64, String, Duration, u64)> {
    let now = Instant::now();
    let Some(timers) = ACTIVE_TIMERS.lock().ok() else {
        return Vec::new();
    };
    timers
        .values()
        .filter(|t| !t.paused)
        .map(|t| {
            (
                t.id,
                t.label.clone(),
                t.deadline.saturating_duration_since(now),
                t.start_ts,
            )
        })
        .collect()
}

/// Return timers that are currently paused.
pub fn paused_timers() -> Vec<(u64, String, Duration, u64)> {
    let Some(timers) = ACTIVE_TIMERS.lock().ok() else {
        return Vec::new();
    };
    timers
        .values()
        .filter(|t| t.paused)
        .map(|t| (t.id, t.label.clone(), t.remaining, t.start_ts))
        .collect()
}

/// Get the start timestamp of the timer with `id`.
pub fn timer_start_ts(id: u64) -> Option<u64> {
    let Some(timers) = ACTIVE_TIMERS.lock().ok() else {
        return None;
    };
    timers.get(&id).map(|t| t.start_ts)
}

/// Return the current pause state of one live timer without performing I/O.
pub fn timer_paused(id: u64) -> Option<bool> {
    ACTIVE_TIMERS
        .lock()
        .ok()?
        .get(&id)
        .map(|timer| timer.paused)
}

/// Cancel the timer with the given `id` if it exists.
pub fn cancel_timer(id: u64) {
    let _ = try_cancel_timer(id);
}

pub fn try_cancel_timer(id: u64) -> TimerMutation {
    let Ok(mut timers) = ACTIVE_TIMERS.lock() else {
        return TimerMutation::Missing;
    };
    let Some(entry) = remove_entry_from_map(&mut timers, id) else {
        return TimerMutation::Missing;
    };
    if entry.persist {
        save_persistent_alarms_locked(&timers);
    }
    TIMER_MANAGER.remove(id);
    TimerMutation::Updated
}

fn remove_entry_from_map(timers: &mut HashMap<u64, TimerEntry>, id: u64) -> Option<TimerEntry> {
    timers.remove(&id)
}

/// Pause the timer with the given `id` if it exists.
pub fn pause_timer(id: u64) {
    let _ = try_pause_timer(id);
}

pub fn try_pause_timer(id: u64) -> TimerMutation {
    let Ok(mut timers) = ACTIVE_TIMERS.lock() else {
        return TimerMutation::Missing;
    };
    let result = pause_entry_in_map(&mut timers, id, Instant::now());
    if result == TimerMutation::Updated {
        if timers.get(&id).is_some_and(|entry| entry.persist) {
            save_persistent_alarms_locked(&timers);
        }
        TIMER_MANAGER.remove(id);
        TIMER_MANAGER.wakeup();
    }
    result
}

fn pause_entry_in_map(
    timers: &mut HashMap<u64, TimerEntry>,
    id: u64,
    now: Instant,
) -> TimerMutation {
    let Some(entry) = timers.get_mut(&id) else {
        return TimerMutation::Missing;
    };
    if entry.paused {
        return TimerMutation::AlreadyPaused;
    }
    entry.remaining = entry.deadline.saturating_duration_since(now);
    entry.paused = true;
    entry.generation = entry.generation.wrapping_add(1);
    TimerMutation::Updated
}

/// Resume the timer with the given `id` if it is paused.
pub fn resume_timer(id: u64) {
    let _ = try_resume_timer(id);
}

pub fn try_resume_timer(id: u64) -> TimerMutation {
    let Ok(mut timers) = ACTIVE_TIMERS.lock() else {
        return TimerMutation::Missing;
    };
    let now = Instant::now();
    let result = resume_entry_in_map(&mut timers, id, now);
    if result == TimerMutation::Updated {
        if let Some(entry) = timers.get(&id) {
            TIMER_MANAGER.register(entry.deadline, entry.id, entry.generation);
        }
        if timers.get(&id).is_some_and(|entry| entry.persist) {
            save_persistent_alarms_locked(&timers);
        }
        TIMER_MANAGER.wakeup();
    }
    result
}

fn resume_entry_in_map(
    timers: &mut HashMap<u64, TimerEntry>,
    id: u64,
    now: Instant,
) -> TimerMutation {
    let Some(entry) = timers.get_mut(&id) else {
        return TimerMutation::Missing;
    };
    if !entry.paused {
        return TimerMutation::AlreadyRunning;
    }
    entry.paused = false;
    entry.deadline = now + entry.remaining;
    entry.generation = entry.generation.wrapping_add(1);
    TimerMutation::Updated
}

fn notify(_msg: &str) {}

fn format_duration(dur: Duration) -> String {
    let secs = dur.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

pub fn format_ts(ts: u64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(ts as i64, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => {
            tracing::warn!("invalid timestamp {ts}");
            match Local.timestamp_opt(0, 0).single() {
                Some(fallback) => fallback.format("%Y-%m-%d %H:%M:%S").to_string(),
                None => "1970-01-01 00:00:00".to_string(),
            }
        }
    }
}

fn allocate_timer_id() -> anyhow::Result<u64> {
    NEXT_ID
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
        .map_err(|_| anyhow::anyhow!("timer id range is exhausted"))
}

fn unix_timestamp(timestamp: i64) -> anyhow::Result<u64> {
    u64::try_from(timestamp)
        .map_err(|_| anyhow::anyhow!("timestamp is outside the supported range"))
}

fn timer_entry_at(
    id: u64,
    duration: Duration,
    name: Option<String>,
    sound: String,
    wall_now: chrono::DateTime<chrono::Local>,
    monotonic_now: Instant,
) -> anyhow::Result<TimerEntry> {
    if duration.is_zero() {
        anyhow::bail!("timer duration must be positive");
    }
    let deadline = monotonic_now
        .checked_add(duration)
        .ok_or_else(|| anyhow::anyhow!("timer duration exceeds the monotonic clock range"))?;
    let start_ts = unix_timestamp(wall_now.timestamp())?;
    let end_ts = start_ts
        .checked_add(duration.as_secs())
        .ok_or_else(|| anyhow::anyhow!("timer end timestamp is outside the supported range"))?;
    Ok(TimerEntry {
        id,
        label: name.unwrap_or_else(|| format!("Timer {duration:?}")),
        deadline,
        persist: false,
        end_ts,
        start_ts,
        paused: false,
        remaining: duration,
        generation: 0,
        sound,
    })
}

fn alarm_entry_at<Tz, Map>(
    id: u64,
    spec: AlarmSpec,
    name: Option<String>,
    sound: String,
    wall_now: chrono::DateTime<Tz>,
    monotonic_now: Instant,
    map_local: Map,
) -> anyhow::Result<TimerEntry>
where
    Tz: chrono::TimeZone,
    Map: FnMut(chrono::NaiveDateTime) -> chrono::LocalResult<chrono::DateTime<Tz>>,
{
    let target = resolve_alarm_spec_at(spec, wall_now.clone(), map_local)?;
    let end_ts = unix_timestamp(target.timestamp())?;
    let start_ts = unix_timestamp(wall_now.timestamp())?;
    let duration = target
        .signed_duration_since(wall_now)
        .to_std()
        .map_err(|_| anyhow::anyhow!("alarm time must be in the future"))?;
    if duration.is_zero() {
        anyhow::bail!("alarm time must be strictly in the future");
    }
    let deadline = monotonic_now
        .checked_add(duration)
        .ok_or_else(|| anyhow::anyhow!("alarm time exceeds the monotonic clock range"))?;
    // Alarm occurrences are constructed at exactly HH:MM:00. Preserve that selected absolute
    // timestamp instead of rebuilding it from floored `start_ts` plus whole seconds.
    Ok(TimerEntry {
        id,
        label: name.unwrap_or_else(|| format!("Alarm {:02}:{:02}", spec.hour, spec.minute)),
        deadline,
        persist: true,
        end_ts,
        start_ts,
        paused: false,
        remaining: duration,
        generation: 0,
        sound,
    })
}

fn publish_entry_into_map(
    timers: &mut HashMap<u64, TimerEntry>,
    entry: TimerEntry,
    persist_before_publish: bool,
    persist_candidate: impl FnOnce(&HashMap<u64, TimerEntry>, &TimerEntry) -> anyhow::Result<()>,
    register: impl FnOnce(Instant, u64, u64),
) -> anyhow::Result<u64> {
    if persist_before_publish {
        persist_candidate(timers, &entry)?;
    }
    let id = entry.id;
    let deadline = entry.deadline;
    let generation = entry.generation;
    timers.insert(id, entry);
    register(deadline, id, generation);
    Ok(id)
}

fn publish_entry(entry: TimerEntry, persist_before_publish: bool) -> anyhow::Result<u64> {
    let mut timers = ACTIVE_TIMERS
        .lock()
        .map_err(|_| anyhow::anyhow!("timer state is unavailable"))?;
    publish_entry_into_map(
        &mut timers,
        entry,
        persist_before_publish,
        |current, candidate| {
            save_persistent_alarms_with_candidate(ALARMS_FILE, current, Some(candidate))
        },
        |deadline, id, generation| TIMER_MANAGER.register(deadline, id, generation),
    )
}

fn publish_restored_entry(entry: TimerEntry) -> anyhow::Result<u64> {
    publish_entry(entry, false)
}

/// Start a timer with checked duration, clock and persistence state.
pub fn try_start_timer_named(
    duration: Duration,
    name: Option<String>,
    sound: String,
) -> anyhow::Result<u64> {
    let wall_now = chrono::Local::now();
    let monotonic_now = Instant::now();
    let id = allocate_timer_id()?;
    let entry = timer_entry_at(id, duration, name, sound, wall_now, monotonic_now)?;
    publish_entry(entry, false)
}

/// Compatibility wrapper for callers that cannot surface a start error.
pub fn start_timer_named(duration: Duration, name: Option<String>, sound: String) {
    if let Err(error) = try_start_timer_named(duration, name, sound) {
        tracing::error!(%error, "timer start failed");
    }
}

/// Start an unnamed timer that lasts `duration`.
pub fn start_timer(duration: Duration, sound: String) {
    start_timer_named(duration, None, sound);
}

/// Start an alarm from a parsed specification using the current local timezone.
pub fn try_start_alarm_named(
    spec: AlarmSpec,
    name: Option<String>,
    sound: String,
) -> anyhow::Result<u64> {
    use chrono::TimeZone;
    let wall_now = chrono::Local::now();
    let monotonic_now = Instant::now();
    let id = allocate_timer_id()?;
    let entry = alarm_entry_at(id, spec, name, sound, wall_now, monotonic_now, |local| {
        chrono::Local.from_local_datetime(&local)
    })?;
    publish_entry(entry, true)
}

/// Compatibility wrapper for the previous hour/minute/date alarm API.
pub fn start_alarm_named(
    hour: u32,
    minute: u32,
    date: Option<chrono::NaiveDate>,
    name: Option<String>,
    sound: String,
) {
    let spec = AlarmSpec {
        hour,
        minute,
        date: date
            .map(AlarmDateSpec::ExplicitDate)
            .unwrap_or(AlarmDateSpec::NextOccurrence),
    };
    if let Err(error) = try_start_alarm_named(spec, name, sound) {
        tracing::error!(%error, "alarm start failed");
    }
}

/// Convenience wrapper for [`start_alarm_named`] without a name.
pub fn start_alarm(hour: u32, minute: u32, date: Option<chrono::NaiveDate>, sound: String) {
    start_alarm_named(hour, minute, date, None, sound);
}

pub struct TimerPlugin;

impl Plugin for TimerPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "timer")
            && rest.is_empty()
        {
            return vec![Action {
                label: "Open timer dialog".into(),
                desc: "Timer".into(),
                action: "timer:dialog:timer".into(),
                args: None,
            }];
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "alarm")
            && rest.is_empty()
        {
            return vec![Action {
                label: "Open alarm dialog".into(),
                desc: "Timer".into(),
                action: "timer:dialog:alarm".into(),
                args: None,
            }];
        }
        if crate::common::strip_prefix_ci(trimmed, "timer list").is_some()
            || crate::common::strip_prefix_ci(trimmed, "alarm list").is_some()
        {
            return active_timers()
                .into_iter()
                .map(|(id, label, rem, _start)| Action {
                    label: format!("{label} ({} left)", format_duration(rem)),
                    desc: "Timer".into(),
                    action: format!("timer:show:{id}"),
                    args: None,
                })
                .collect();
        }
        if crate::common::strip_prefix_ci(trimmed, "timer cancel").is_some() {
            return active_timers()
                .into_iter()
                .map(|(id, label, _rem, _start)| Action {
                    label: format!("Cancel {label}"),
                    desc: "Timer".into(),
                    action: format!("timer:cancel:{id}"),
                    args: None,
                })
                .collect();
        }
        if crate::common::strip_prefix_ci(trimmed, "timer rm").is_some() {
            return active_timers()
                .into_iter()
                .map(|(id, label, rem, _start)| Action {
                    label: format!("Remove {label} ({} left)", format_duration(rem)),
                    desc: "Timer".into(),
                    action: format!("timer:cancel:{id}"),
                    args: None,
                })
                .collect();
        }
        if let Some(id_str) = crate::common::strip_prefix_ci(trimmed, "timer pause") {
            let tail = id_str.trim();
            if tail.is_empty() {
                return running_timers()
                    .into_iter()
                    .map(|(id, label, _rem, _start)| Action {
                        label: format!("Pause {label}"),
                        desc: "Timer".into(),
                        action: format!("timer:pause:{id}"),
                        args: None,
                    })
                    .collect();
            } else if let Ok(id) = tail.parse::<u64>() {
                return vec![Action {
                    label: format!("Pause timer {id}"),
                    desc: "Timer".into(),
                    action: format!("timer:pause:{id}"),
                    args: None,
                }];
            }
        }
        if let Some(id_str) = crate::common::strip_prefix_ci(trimmed, "timer resume") {
            let tail = id_str.trim();
            if tail.is_empty() {
                return paused_timers()
                    .into_iter()
                    .map(|(id, label, _rem, _start)| Action {
                        label: format!("Resume {label}"),
                        desc: "Timer".into(),
                        action: format!("timer:resume:{id}"),
                        args: None,
                    })
                    .collect();
            } else if let Ok(id) = tail.parse::<u64>() {
                return vec![Action {
                    label: format!("Resume timer {id}"),
                    desc: "Timer".into(),
                    action: format!("timer:resume:{id}"),
                    args: None,
                }];
            }
        }
        if let Some(arg) = crate::common::strip_prefix_ci(trimmed, "timer add") {
            let arg = arg.trim();
            let mut parts = arg.splitn(2, ' ');
            let dur_part = parts.next().unwrap_or("");
            if parse_duration(dur_part).is_some() {
                let name_part = parts.next();
                let action = if let Some(name) = name_part {
                    format!("timer:start:{dur_part}|{name}")
                } else {
                    format!("timer:start:{dur_part}")
                };
                let label = if let Some(name) = name_part {
                    format!("Start timer {dur_part} {name}")
                } else {
                    format!("Start timer {dur_part}")
                };
                return vec![Action {
                    label,
                    desc: "Timer".into(),
                    action,
                    args: None,
                }];
            }
        }
        if let Some(arg) = crate::common::strip_prefix_ci(trimmed, "alarm ") {
            let arg = arg.trim();
            if let Some((_spec, time_part, name_part)) = parse_alarm_search_input(arg) {
                let action = if let Some(name) = name_part {
                    format!("alarm:set:{time_part}|{name}")
                } else {
                    format!("alarm:set:{time_part}")
                };
                let label = if let Some(name) = name_part {
                    format!("Set alarm {time_part} {name}")
                } else {
                    format!("Set alarm {time_part}")
                };
                return vec![Action {
                    label,
                    desc: "Timer".into(),
                    action,
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "timer"
    }

    fn description(&self) -> &str {
        "Create timers and alarms (prefix: `timer` / `alarm`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search", "completion_dialog"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "timer".into(),
                desc: "Timer".into(),
                action: "query:timer".into(),
                args: None,
            },
            Action {
                label: "timer add".into(),
                desc: "Timer".into(),
                action: "query:timer add ".into(),
                args: None,
            },
            Action {
                label: "timer list".into(),
                desc: "Timer".into(),
                action: "query:timer list".into(),
                args: None,
            },
            Action {
                label: "timer pause".into(),
                desc: "Timer".into(),
                action: "query:timer pause".into(),
                args: None,
            },
            Action {
                label: "timer resume".into(),
                desc: "Timer".into(),
                action: "query:timer resume".into(),
                args: None,
            },
            Action {
                label: "timer cancel".into(),
                desc: "Timer".into(),
                action: "query:timer cancel".into(),
                args: None,
            },
            Action {
                label: "timer rm".into(),
                desc: "Timer".into(),
                action: "query:timer rm".into(),
                args: None,
            },
            Action {
                label: "alarm".into(),
                desc: "Timer".into(),
                action: "query:alarm".into(),
                args: None,
            },
        ]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["timer", "alarm"]
    }
}

fn parse_alarm_search_input(input: &str) -> Option<(AlarmSpec, &str, Option<&str>)> {
    let input = input.trim();
    let first = input.split_whitespace().next()?;
    let has_date_prefix = first
        .strip_suffix(['d', 'D'])
        .is_some_and(|days| days.parse::<i64>().is_ok())
        || chrono::NaiveDate::parse_from_str(first, "%Y-%m-%d").is_ok();
    let words_to_consume = if has_date_prefix { 2 } else { 1 };

    let mut consumed = 0;
    let mut words = input.split_whitespace();
    for _ in 0..words_to_consume {
        let word = words.next()?;
        let relative_end = input[consumed..].find(word)? + word.len();
        consumed += relative_end;
        while input[consumed..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            consumed += input[consumed..].chars().next()?.len_utf8();
        }
    }

    let time_part = input[..consumed].trim();
    let spec = parse_alarm_spec(time_part).ok()?;
    let name = input[consumed..].trim();
    Some((spec, time_part, (!name.is_empty()).then_some(name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, LocalResult, NaiveDate, TimeZone, Utc};

    fn utc_datetime(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        nanos: u32,
    ) -> chrono::DateTime<Utc> {
        Utc.from_utc_datetime(
            &NaiveDate::from_ymd_opt(year, month, day)
                .unwrap()
                .and_hms_nano_opt(hour, minute, second, nanos)
                .unwrap(),
        )
    }

    fn utc_map(local: chrono::NaiveDateTime) -> LocalResult<chrono::DateTime<Utc>> {
        Utc.from_local_datetime(&local)
    }

    fn test_entry(id: u64, persist: bool) -> TimerEntry {
        let now = Instant::now();
        TimerEntry {
            id,
            label: "alarm test".into(),
            deadline: now + Duration::from_secs(60),
            persist,
            end_ts: 1_900_000_060,
            start_ts: 1_900_000_000,
            paused: false,
            remaining: Duration::from_secs(60),
            generation: 0,
            sound: "Bell".into(),
        }
    }

    #[test]
    fn missing_alarms_initialize_but_malformed_store_rejects_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("alarms.json");
        let path_str = path.to_str().unwrap();
        save_persistent_alarms_to(path_str, &HashMap::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[]");

        let invalid = b"not alarms";
        std::fs::write(&path, invalid).unwrap();
        assert!(save_persistent_alarms_to(path_str, &HashMap::new()).is_err());
        assert!(
            save_persistent_alarms_with_candidate(
                path_str,
                &HashMap::new(),
                Some(&test_entry(90, true)),
            )
            .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn timer_pause_resume_and_cancel_preserve_heap_generation_without_worker() {
        let id = 93;
        let mut timers = HashMap::from([(id, test_entry(id, false))]);
        let mut heap = TimerHeap::new();
        let entry = timers.get(&id).unwrap();
        heap.push(HeapEntry {
            deadline: entry.deadline,
            id,
            generation: entry.generation,
        });
        assert_eq!(heap.len(), 1);

        assert_eq!(
            pause_entry_in_map(&mut timers, id, Instant::now()),
            TimerMutation::Updated
        );
        assert_eq!(timers[&id].generation, 1);
        heap.remove(id);
        assert_eq!(heap.len(), 0);
        assert_eq!(
            pause_entry_in_map(&mut timers, id, Instant::now()),
            TimerMutation::AlreadyPaused
        );

        assert_eq!(
            resume_entry_in_map(&mut timers, id, Instant::now()),
            TimerMutation::Updated
        );
        let resumed = &timers[&id];
        assert_eq!(resumed.generation, 2);
        heap.push(HeapEntry {
            deadline: resumed.deadline,
            id,
            generation: resumed.generation,
        });
        assert_eq!(heap.len(), 1);
        assert_eq!(
            resume_entry_in_map(&mut timers, id, Instant::now()),
            TimerMutation::AlreadyRunning
        );

        assert!(remove_entry_from_map(&mut timers, id).is_some());
        heap.remove(id);
        assert!(timers.is_empty());
        assert_eq!(heap.len(), 0);
    }

    #[test]
    fn duration_parser_checks_arithmetic_and_rejects_zero() {
        assert_eq!(parse_duration("1:02"), Some(Duration::from_secs(62)));
        assert_eq!(parse_duration("1:02:03"), Some(Duration::from_secs(3_723)));
        assert_eq!(parse_duration("2h"), Some(Duration::from_secs(7_200)));
        assert_eq!(parse_duration("0s"), None);
        assert_eq!(parse_duration("0:00"), None);
        assert_eq!(parse_duration("-1s"), None);
        assert_eq!(parse_duration("5🍵"), None);
        assert_eq!(parse_duration("18446744073709551615h"), None);
        assert_eq!(parse_duration("18446744073709551615:59:59"), None);
        assert_eq!(parse_duration("1:60"), None);
        assert_eq!(parse_duration("1:02:60"), None);
    }

    #[test]
    fn alarm_spec_parsing_is_clock_independent_and_preserves_date_forms() {
        assert_eq!(
            parse_alarm_spec("07:30").unwrap(),
            AlarmSpec {
                hour: 7,
                minute: 30,
                date: AlarmDateSpec::NextOccurrence,
            }
        );
        assert_eq!(
            parse_alarm_spec("-2d 07:30").unwrap().date,
            AlarmDateSpec::DayOffset(-2)
        );
        assert_eq!(
            parse_alarm_spec("2024-01-31 05:15").unwrap().date,
            AlarmDateSpec::ExplicitDate(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())
        );
        for invalid in [
            "24:00",
            "12:60",
            "2024-02-30 12:00",
            "1d 12:00 trailing",
            "9223372036854775808d 12:00",
        ] {
            assert!(parse_alarm_spec(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn legacy_parse_hhmm_adapter_preserves_plain_date_and_injected_offset_forms() {
        assert_eq!(parse_hhmm("07:30"), Some((7, 30, None)));
        assert_eq!(
            parse_hhmm("2024-01-31 05:15"),
            Some((5, 15, Some(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())))
        );
        let offset = parse_alarm_spec("2d 07:30").unwrap();
        assert_eq!(
            alarm_spec_legacy_parts(offset, Some(NaiveDate::from_ymd_opt(2024, 1, 30).unwrap())),
            Some((7, 30, Some(NaiveDate::from_ymd_opt(2024, 2, 1).unwrap())))
        );
        let out_of_range = parse_alarm_spec("1000000000d 12:00").unwrap();
        assert_eq!(
            alarm_spec_legacy_parts(
                out_of_range,
                Some(NaiveDate::from_ymd_opt(2024, 1, 30).unwrap())
            ),
            None
        );
    }

    #[test]
    fn next_alarm_is_strictly_future_at_exact_minute_and_subsecond_boundaries() {
        let spec = parse_alarm_spec("12:04").unwrap();
        let before = utc_datetime(2025, 4, 3, 12, 3, 59, 999_999_999);
        let same_day = resolve_alarm_spec_at(spec, before, utc_map).unwrap();
        assert_eq!(same_day, utc_datetime(2025, 4, 3, 12, 4, 0, 0));

        for now in [
            utc_datetime(2025, 4, 3, 12, 4, 0, 0),
            utc_datetime(2025, 4, 3, 12, 4, 59, 999_999_999),
        ] {
            let next_day = resolve_alarm_spec_at(spec, now, utc_map).unwrap();
            assert_eq!(next_day, utc_datetime(2025, 4, 4, 12, 4, 0, 0));
        }
    }

    #[test]
    fn day_offsets_are_relative_to_injected_local_date_and_explicit_dates_refuse_past() {
        let now = utc_datetime(2025, 4, 3, 23, 59, 59, 500_000_000);
        let offset =
            resolve_alarm_spec_at(parse_alarm_spec("1d 00:00").unwrap(), now, utc_map)
                .unwrap();
        assert_eq!(offset, utc_datetime(2025, 4, 4, 0, 0, 0, 0));

        let yesterday = parse_alarm_spec("-1d 23:59").unwrap();
        assert!(resolve_alarm_spec_at(yesterday, now, utc_map).is_err());
        let past = parse_alarm_spec("2025-04-03 23:59").unwrap();
        assert!(resolve_alarm_spec_at(past, now, utc_map).is_err());
        let future = parse_alarm_spec("2025-04-04 01:00").unwrap();
        assert_eq!(
            resolve_alarm_spec_at(future, now, utc_map).unwrap(),
            utc_datetime(2025, 4, 4, 1, 0, 0, 0)
        );
        let overflowing = AlarmSpec {
            hour: 12,
            minute: 0,
            date: AlarmDateSpec::DayOffset(i64::MAX),
        };
        assert!(resolve_alarm_spec_at(overflowing, now, utc_map).is_err());
    }

    #[test]
    fn ambiguous_alarm_selects_earliest_strictly_future_fold_candidate() {
        let date = NaiveDate::from_ymd_opt(2025, 10, 26).unwrap();
        let spec = parse_alarm_spec("01:30").unwrap();
        let early_offset = FixedOffset::east_opt(7_200).unwrap();
        let late_offset = FixedOffset::east_opt(3_600).unwrap();
        let local = date.and_hms_opt(1, 30, 0).unwrap();
        let first = early_offset.from_local_datetime(&local).single().unwrap();
        let second = late_offset.from_local_datetime(&local).single().unwrap();
        let between = first + chrono::Duration::minutes(5);
        let resolved = resolve_alarm_spec_at(spec, between, |_| {
            LocalResult::Ambiguous(first, second)
        })
        .unwrap();
        assert_eq!(resolved, second);

        let before = first - chrono::Duration::minutes(5);
        let resolved = resolve_alarm_spec_at(spec, before, |_| {
            LocalResult::Ambiguous(second, first)
        })
        .unwrap();
        assert_eq!(resolved, first);

        let after = second + chrono::Duration::minutes(1);
        let next_day_local = NaiveDate::from_ymd_opt(2025, 10, 27)
            .unwrap()
            .and_hms_opt(1, 30, 0)
            .unwrap();
        let next_day = early_offset
            .from_local_datetime(&next_day_local)
            .single()
            .unwrap();
        let resolved = resolve_alarm_spec_at(spec, after, |local| {
            if local.date() == date {
                LocalResult::Ambiguous(first, second)
            } else {
                early_offset.from_local_datetime(&local)
            }
        })
        .unwrap();
        assert_eq!(resolved, next_day);
    }

    #[test]
    fn next_occurrence_skips_gap_but_explicit_gap_is_rejected() {
        let now = utc_datetime(2025, 3, 30, 1, 0, 0, 0);
        let spec = parse_alarm_spec("02:30").unwrap();
        let tomorrow = utc_datetime(2025, 3, 31, 2, 30, 0, 0);
        let next = resolve_alarm_spec_at(spec, now, |local| {
            if local.date() == now.date_naive() {
                LocalResult::None
            } else {
                Utc.from_local_datetime(&local)
            }
        })
        .unwrap();
        assert_eq!(next, tomorrow);

        let explicit = parse_alarm_spec("2025-03-30 02:30").unwrap();
        assert!(resolve_alarm_spec_at(explicit, now, |_| LocalResult::None).is_err());
    }

    #[test]
    fn alarm_deadline_and_saved_timestamp_keep_exact_target_with_subsecond_start() {
        let offset = FixedOffset::east_opt(0).unwrap();
        let local_now = NaiveDate::from_ymd_opt(2025, 4, 3)
            .unwrap()
            .and_hms_nano_opt(12, 3, 59, 500_000_000)
            .unwrap();
        let now = offset.from_local_datetime(&local_now).single().unwrap();
        let monotonic_now = Instant::now();
        let spec = parse_alarm_spec("12:04").unwrap();
        let entry = alarm_entry_at(
            77,
            spec,
            Some("work alarm".into()),
            "Bell".into(),
            now,
            monotonic_now,
            |local| offset.from_local_datetime(&local),
        )
        .unwrap();
        let target = offset
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2025, 4, 3)
                    .unwrap()
                    .and_hms_opt(12, 4, 0)
                    .unwrap(),
            )
            .single()
            .unwrap();
        assert_eq!(entry.end_ts, target.timestamp() as u64);
        assert_eq!(entry.remaining, Duration::from_millis(500));
        assert_eq!(
            entry.deadline.duration_since(monotonic_now),
            entry.remaining
        );

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("alarms.json");
        let path = path.to_str().unwrap();
        save_persistent_alarms_with_candidate(path, &HashMap::new(), Some(&entry)).unwrap();
        let alarms = match load_json::<Vec<SavedAlarm>>(path).unwrap() {
            LoadState::Loaded(alarms) => alarms,
            _ => panic!("unexpected alarm store state"),
        };
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].end_ts, target.timestamp() as u64);
        assert_eq!(alarms[0].label, "work alarm");
        assert_eq!(alarms[0].sound, "Bell");

        let utc_now = now.with_timezone(&Utc);
        assert_eq!(
            remaining_until_timestamp(alarms[0].end_ts, &utc_now),
            Some(Duration::from_millis(500))
        );
        let reloaded =
            load_saved_alarm_entries(path, now.with_timezone(&chrono::Local), monotonic_now)
                .unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].end_ts, target.timestamp() as u64);
        assert_eq!(reloaded[0].remaining, Duration::from_millis(500));
        assert_eq!(
            reloaded[0].deadline.duration_since(monotonic_now),
            Duration::from_millis(500)
        );
        assert_eq!(reloaded[0].label, "work alarm");
        assert_eq!(reloaded[0].sound, "Bell");
    }

    #[test]
    fn persistence_failure_does_not_publish_active_entry_or_heap_registration() {
        use std::cell::Cell;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("alarms.json");
        let invalid_store = b"preserve this malformed store";
        std::fs::write(&path, invalid_store).unwrap();
        let path = path.to_str().unwrap();
        let mut active = HashMap::new();
        let registered = Cell::new(false);
        let result = publish_entry_into_map(
            &mut active,
            test_entry(92, true),
            true,
            |current, candidate| {
                save_persistent_alarms_with_candidate(path, current, Some(candidate))
            },
            |_, _, _| registered.set(true),
        );
        assert!(result.is_err());
        assert!(active.is_empty());
        assert!(!registered.get());
        assert_eq!(std::fs::read(path).unwrap(), invalid_store);
    }

    #[test]
    fn valid_alarm_store_write_failure_does_not_publish_candidate() {
        use std::cell::Cell;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("alarms.json");
        let original =
            br#"[{"label":"existing","end_ts":1900000000,"start_ts":1800000000,"sound":"Bell"}]"#;
        std::fs::write(&path, original).unwrap();
        let path = path.to_str().unwrap();
        let write_reached = Cell::new(false);
        let registered = Cell::new(false);
        let mut active = HashMap::new();

        let result = publish_entry_into_map(
            &mut active,
            test_entry(94, true),
            true,
            |current, candidate| {
                save_persistent_alarms_with_candidate_using(
                    path,
                    current,
                    Some(candidate),
                    |write_path, proposed| {
                        write_reached.set(true);
                        assert_eq!(write_path, path);
                        assert_eq!(proposed.len(), 1);
                        assert_eq!(proposed[0].label, "alarm test");
                        anyhow::bail!("injected write failure")
                    },
                )
            },
            |_, _, _| registered.set(true),
        );

        assert!(result.is_err());
        assert!(write_reached.get(), "valid JSON reached the write boundary");
        assert!(active.is_empty());
        assert!(!registered.get());
        assert_eq!(std::fs::read(path).unwrap(), original);
    }

    #[test]
    fn checked_timer_entry_rejects_programmatic_zero_and_deadline_overflow() {
        let wall_now = chrono::Local::now();
        let monotonic_now = Instant::now();
        assert!(
            timer_entry_at(
                1,
                Duration::ZERO,
                None,
                "None".into(),
                wall_now,
                monotonic_now,
            )
            .is_err()
        );
        assert!(
            timer_entry_at(
                2,
                Duration::MAX,
                None,
                "None".into(),
                wall_now,
                monotonic_now,
            )
            .is_err()
        );
    }
}
