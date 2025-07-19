use crate::actions::Action;
use crate::plugin::Plugin;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

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
    let list: Vec<SavedAlarm> = timers
        .values()
        .filter(|t| t.persist)
        .map(|t| SavedAlarm {
            label: t.label.clone(),
            end_ts: t.end_ts,
            start_ts: t.start_ts,
            sound: t.sound.clone(),
        })
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(ALARMS_FILE, json);
    }
}

pub static ACTIVE_TIMERS: Lazy<Mutex<HashMap<u64, TimerEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

struct TimerManager {
    inner: Arc<Inner>,
}

struct Inner {
    heap: Mutex<BinaryHeap<Reverse<(Instant, u64, u64)>>>,
    condvar: Condvar,
}

impl TimerManager {
    fn new() -> Self {
        let inner = Arc::new(Inner {
            heap: Mutex::new(BinaryHeap::new()),
            condvar: Condvar::new(),
        });
        let thread_inner = inner.clone();
        thread::spawn(move || Self::run(thread_inner));
        Self { inner }
    }

    fn run(inner: Arc<Inner>) {
        let mut heap_guard = inner.heap.lock().unwrap();
        loop {
            while heap_guard.peek().is_none() {
                heap_guard = inner.condvar.wait(heap_guard).unwrap();
            }
            let now = Instant::now();
            if let Some(Reverse((deadline, id, gen))) = heap_guard.peek().cloned() {
                if deadline <= now {
                    heap_guard.pop();
                    drop(heap_guard);
                    Self::fire_timer(id, gen);
                    heap_guard = inner.heap.lock().unwrap();
                    continue;
                } else {
                    let wait = deadline.saturating_duration_since(now);
                    let res = inner.condvar.wait_timeout(heap_guard, wait).unwrap();
                    heap_guard = res.0;
                    continue;
                }
            }
        }
    }

    fn fire_timer(id: u64, gen: u64) {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        if let Some(entry) = list.get(&id) {
            if entry.generation == gen && !entry.paused {
                let entry = list.remove(&id).unwrap();
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
                FINISHED_MESSAGES.lock().unwrap().push(msg);
            }
        }
    }

    fn register(&self, deadline: Instant, id: u64, gen: u64) {
        let mut heap = self.inner.heap.lock().unwrap();
        heap.push(Reverse((deadline, id, gen)));
        self.inner.condvar.notify_one();
    }

    fn wakeup(&self) {
        self.inner.condvar.notify_one();
    }
}

static TIMER_MANAGER: Lazy<TimerManager> = Lazy::new(|| TimerManager::new());

/// Reset the flag tracking whether saved alarms have been loaded.
pub fn reset_alarms_loaded() {
    ALARMS_LOADED.store(false, Ordering::SeqCst);
}

/// Retrieve and clear finished timer/alarm notifications.
pub fn take_finished_messages() -> Vec<String> {
    let mut list = FINISHED_MESSAGES.lock().unwrap();
    let out = list.clone();
    list.clear();
    out
}

/// Load persisted alarms from the alarms file if not already loaded.
pub fn load_saved_alarms() {
    if ALARMS_LOADED.load(Ordering::SeqCst) {
        return;
    }
    let content = std::fs::read_to_string(ALARMS_FILE).unwrap_or_default();
    if content.is_empty() {
        return;
    }
    let list: Vec<SavedAlarm> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to parse alarms file: {e}");
            return;
        }
    };
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for alarm in list {
        if alarm.end_ts > now {
            let dur = Duration::from_secs(alarm.end_ts - now);
            let start_ts = if alarm.start_ts > 0 {
                alarm.start_ts
            } else {
                alarm.end_ts - dur.as_secs()
            };
            start_entry(dur, alarm.label, true, start_ts, alarm.end_ts, alarm.sound);
        }
    }
    ALARMS_LOADED.store(true, Ordering::SeqCst);
}

/// Parse a duration string.
///
/// Supports `Ns`, `Nm`, `Nh` as well as `hh:mm:ss` and `mm:ss` formats.
pub fn parse_duration(input: &str) -> Option<Duration> {
    if input.contains(':') {
        let parts: Vec<&str> = input.split(':').collect();
        if parts.len() == 2 || parts.len() == 3 {
            let mut nums = parts.iter().map(|p| p.parse::<u64>().ok());
            let (h, m, s) = if parts.len() == 3 {
                (nums.next()?, nums.next()?, nums.next()?)
            } else {
                (Some(0), nums.next()?, nums.next()?)
            };
            let (h, m, s) = (h?, m?, s?);
            if m < 60 && s < 60 {
                return Some(Duration::from_secs(h * 3600 + m * 60 + s));
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    if input.len() < 2 {
        return None;
    }
    let (num_str, unit) = input.split_at(input.len() - 1);
    let value: u64 = num_str.parse().ok()?;
    match unit {
        "s" | "S" => Some(Duration::from_secs(value)),
        "m" | "M" => Some(Duration::from_secs(value * 60)),
        "h" | "H" => Some(Duration::from_secs(value * 60 * 60)),
        _ => None,
    }
}

/// Parse a time of day in `HH:MM` format.
pub fn parse_hhmm(input: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = input.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    if h < 24 && m < 60 {
        Some((h, m))
    } else {
        None
    }
}

/// Return a list of active timers with remaining time.
pub fn active_timers() -> Vec<(u64, String, Duration, u64)> {
    let now = Instant::now();
    ACTIVE_TIMERS
        .lock()
        .unwrap()
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
    ACTIVE_TIMERS
        .lock()
        .unwrap()
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
    ACTIVE_TIMERS
        .lock()
        .unwrap()
        .values()
        .filter(|t| t.paused)
        .map(|t| (t.id, t.label.clone(), t.remaining, t.start_ts))
        .collect()
}

/// Get the start timestamp of the timer with `id`.
pub fn timer_start_ts(id: u64) -> Option<u64> {
    let timers = ACTIVE_TIMERS.lock().unwrap();
    timers.get(&id).map(|t| t.start_ts)
}

/// Cancel the timer with the given `id` if it exists.
pub fn cancel_timer(id: u64) {
    let mut timers = ACTIVE_TIMERS.lock().unwrap();
    if let Some(entry) = timers.remove(&id) {
        if entry.persist {
            save_persistent_alarms_locked(&timers);
        }
        TIMER_MANAGER.wakeup();
    }
}

/// Pause the timer with the given `id` if it exists.
pub fn pause_timer(id: u64) {
    let mut timers = ACTIVE_TIMERS.lock().unwrap();
    if let Some(t) = timers.get_mut(&id) {
        if !t.paused {
            t.remaining = t.deadline.saturating_duration_since(Instant::now());
            t.paused = true;
            t.generation = t.generation.wrapping_add(1);
            if t.persist {
                save_persistent_alarms_locked(&timers);
            }
            TIMER_MANAGER.wakeup();
        }
    }
}

/// Resume the timer with the given `id` if it is paused.
pub fn resume_timer(id: u64) {
    let mut timers = ACTIVE_TIMERS.lock().unwrap();
    if let Some(t) = timers.get_mut(&id) {
        if t.paused {
            t.paused = false;
            t.deadline = Instant::now() + t.remaining;
            t.generation = t.generation.wrapping_add(1);
            TIMER_MANAGER.register(t.deadline, t.id, t.generation);
            if t.persist {
                save_persistent_alarms_locked(&timers);
            }
            TIMER_MANAGER.wakeup();
        }
    }
}

#[cfg(feature = "notify")]
fn notify(msg: &str) {
    let _ = notify_rust::Notification::new()
        .summary("Multi Launcher")
        .body(msg)
        .show();
}

#[cfg(not(feature = "notify"))]
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
    Local
        .timestamp_opt(ts as i64, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).single().unwrap())
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn start_entry(
    duration: Duration,
    label: String,
    persist: bool,
    start_ts: u64,
    end_ts: u64,
    sound: String,
) {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let deadline = Instant::now() + duration;
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.insert(
            id,
            TimerEntry {
                id,
                label: label.clone(),
                deadline,
                persist,
                end_ts,
                start_ts,
                paused: false,
                remaining: duration,
                generation: 0,
                sound: sound.clone(),
            },
        );
        if persist {
            save_persistent_alarms_locked(&list);
        }
    }
    TIMER_MANAGER.register(deadline, id, 0);
}

/// Start a timer that lasts `duration` with an optional `name`.
pub fn start_timer_named(duration: Duration, name: Option<String>, sound: String) {
    let label = name.unwrap_or_else(|| format!("Timer {:?}", duration));
    let start_ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let end_ts = start_ts + duration.as_secs();
    start_entry(duration, label, false, start_ts, end_ts, sound);
}

/// Start an unnamed timer that lasts `duration`.
pub fn start_timer(duration: Duration, sound: String) {
    start_timer_named(duration, None, sound);
}

/// Set an alarm for the specified time with an optional `name`.
pub fn start_alarm_named(hour: u32, minute: u32, name: Option<String>, sound: String) {
    use chrono::{Duration as ChronoDuration, Local};
    let now = Local::now();
    let mut target = now.date_naive().and_hms_opt(hour, minute, 0).unwrap();
    if target <= now.naive_local() {
        target += ChronoDuration::days(1);
    }
    let duration = (target - now.naive_local())
        .to_std()
        .unwrap_or(Duration::from_secs(0));
    let label = name.unwrap_or_else(|| format!("Alarm {:02}:{:02}", hour, minute));
    let start_ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let end_ts = start_ts + duration.as_secs();
    start_entry(duration, label, true, start_ts, end_ts, sound);
}

/// Convenience wrapper for [`start_alarm_named`] without a name.
pub fn start_alarm(hour: u32, minute: u32, sound: String) {
    start_alarm_named(hour, minute, None, sound);
}

pub struct TimerPlugin;

impl Plugin for TimerPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "timer") {
            if rest.is_empty() {
            return vec![Action {
                label: "Open timer dialog".into(),
                desc: "Timer".into(),
                action: "timer:dialog:timer".into(),
                args: None,
            }];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "alarm") {
            if rest.is_empty() {
            return vec![Action {
                label: "Open alarm dialog".into(),
                desc: "Timer".into(),
                action: "timer:dialog:alarm".into(),
                args: None,
            }];
            }
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
            let mut parts = arg.splitn(2, ' ');
            let time_part = parts.next().unwrap_or("");
            if parse_hhmm(time_part).is_some() {
                let name_part = parts.next();
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
            Action { label: "timer".into(), desc: "Timer".into(), action: "query:timer".into(), args: None },
            Action { label: "timer add".into(), desc: "Timer".into(), action: "query:timer add ".into(), args: None },
            Action { label: "timer list".into(), desc: "Timer".into(), action: "query:timer list".into(), args: None },
            Action { label: "timer pause".into(), desc: "Timer".into(), action: "query:timer pause".into(), args: None },
            Action { label: "timer resume".into(), desc: "Timer".into(), action: "query:timer resume".into(), args: None },
            Action { label: "timer cancel".into(), desc: "Timer".into(), action: "query:timer cancel".into(), args: None },
            Action { label: "timer rm".into(), desc: "Timer".into(), action: "query:timer rm".into(), args: None },
            Action { label: "alarm".into(), desc: "Timer".into(), action: "query:alarm".into(), args: None },
        ]
    }
}
