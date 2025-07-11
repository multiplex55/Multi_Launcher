use crate::actions::Action;
use crate::plugin::Plugin;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use std::thread;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub const ALARMS_FILE: &str = "alarms.json";
pub static FINISHED_MESSAGES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub struct TimerEntry {
    pub id: u64,
    pub label: String,
    pub deadline: Instant,
    pub cancel: Arc<AtomicBool>,
    pub persist: bool,
    pub end_ts: u64,
}

#[derive(Serialize, Deserialize)]
struct SavedAlarm {
    label: String,
    end_ts: u64,
}

fn save_persistent_alarms_locked(timers: &Vec<TimerEntry>) {
    let list: Vec<SavedAlarm> = timers
        .iter()
        .filter(|t| t.persist)
        .map(|t| SavedAlarm {
            label: t.label.clone(),
            end_ts: t.end_ts,
        })
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(ALARMS_FILE, json);
    }
}

pub static ACTIVE_TIMERS: Lazy<Mutex<Vec<TimerEntry>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn take_finished_messages() -> Vec<String> {
    let mut list = FINISHED_MESSAGES.lock().unwrap();
    let out = list.clone();
    list.clear();
    out
}

pub fn load_saved_alarms() {
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
            start_entry(dur, alarm.label, true, alarm.end_ts);
        }
    }
}

pub fn parse_duration(input: &str) -> Option<Duration> {
    if input.len() < 2 { return None; }
    let (num_str, unit) = input.split_at(input.len()-1);
    let value: u64 = num_str.parse().ok()?;
    match unit {
        "s" | "S" => Some(Duration::from_secs(value)),
        "m" | "M" => Some(Duration::from_secs(value * 60)),
        "h" | "H" => Some(Duration::from_secs(value * 60 * 60)),
        _ => None,
    }
}

pub fn parse_hhmm(input: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = input.split(':').collect();
    if parts.len() != 2 { return None; }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    if h < 24 && m < 60 {
        Some((h, m))
    } else {
        None
    }
}

pub fn active_timers() -> Vec<(u64, String, Duration)> {
    let now = Instant::now();
    ACTIVE_TIMERS
        .lock()
        .unwrap()
        .iter()
        .map(|t| (t.id, t.label.clone(), t.deadline.saturating_duration_since(now)))
        .collect()
}

pub fn cancel_timer(id: u64) {
    let mut timers = ACTIVE_TIMERS.lock().unwrap();
    if let Some(pos) = timers.iter().position(|t| t.id == id) {
        let entry = timers.remove(pos);
        entry.cancel.store(true, Ordering::SeqCst);
        if entry.persist {
            save_persistent_alarms_locked(&timers);
        }
    }
}

fn notify(msg: &str) {
    #[allow(unused)]
    {
        let _ = notify_rust::Notification::new().summary("Multi Launcher").body(msg).show();
    }
}

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

fn start_entry(duration: Duration, label: String, persist: bool, end_ts: u64) {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    let deadline = Instant::now() + duration;
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.push(TimerEntry {
            id,
            label: label.clone(),
            deadline,
            cancel: cancel.clone(),
            persist,
            end_ts,
        });
        if persist {
            save_persistent_alarms_locked(&list);
        }
    }
    thread::spawn(move || {
        let start = Instant::now();
        loop {
            let remaining = duration.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                break;
            }
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !cancel.load(Ordering::SeqCst) {
            let msg = if persist {
                format!("Alarm triggered: {}", label)
            } else {
                format!("Timer finished: {}", label)
            };
            notify(&msg);
            FINISHED_MESSAGES.lock().unwrap().push(msg);
        }
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        if let Some(pos) = list.iter().position(|t| t.id == id) {
            let removed = list.remove(pos);
            if removed.persist {
                save_persistent_alarms_locked(&list);
            }
        }
    });
}

pub fn start_timer_named(duration: Duration, name: Option<String>) {
    let label = name.unwrap_or_else(|| format!("Timer {:?}", duration));
    let end_ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + duration.as_secs();
    start_entry(duration, label, false, end_ts);
}

pub fn start_timer(duration: Duration) {
    start_timer_named(duration, None);
}

pub fn start_alarm_named(hour: u32, minute: u32, name: Option<String>) {
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
    let end_ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + duration.as_secs();
    start_entry(duration, label, true, end_ts);
}

pub fn start_alarm(hour: u32, minute: u32) {
    start_alarm_named(hour, minute, None);
}

pub struct TimerPlugin;

impl Plugin for TimerPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("timer") {
            return vec![Action {
                label: "Open timer dialog".into(),
                desc: "Timer".into(),
                action: "timer:dialog:timer".into(),
                args: None,
            }];
        }
        if trimmed.eq_ignore_ascii_case("alarm") {
            return vec![Action {
                label: "Open alarm dialog".into(),
                desc: "Timer".into(),
                action: "timer:dialog:alarm".into(),
                args: None,
            }];
        }
        if trimmed.starts_with("timer list") || trimmed.starts_with("alarm list") {
            return active_timers()
                .into_iter()
                .map(|(id, label, rem)| Action {
                    label: format!("{label} ({} left)", format_duration(rem)),
                    desc: "Timer".into(),
                    action: format!("timer:show:{id}"),
                    args: None,
                })
                .collect();
        }
        if trimmed.starts_with("timer cancel") {
            return active_timers()
                .into_iter()
                .map(|(id, label, _)| Action {
                    label: format!("Cancel {label}"),
                    desc: "Timer".into(),
                    action: format!("timer:cancel:{id}"),
                    args: None,
                })
                .collect();
        }
        if let Some(arg) = trimmed.strip_prefix("timer ") {
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
                return vec![Action { label, desc: "Timer".into(), action, args: None }];
            }
        }
        if let Some(arg) = trimmed.strip_prefix("alarm ") {
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
                return vec![Action { label, desc: "Timer".into(), action, args: None }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str { "timer" }

    fn description(&self) -> &str { "Create timers and alarms (prefix: `timer` / `alarm`)" }

    fn capabilities(&self) -> &[&str] { &["search", "completion_dialog"] }
}

