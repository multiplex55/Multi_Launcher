use crate::actions::Action;
use crate::plugin::Plugin;
use once_cell::sync::Lazy;
use std::sync::{Mutex, Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::{Duration, Instant};
use std::thread;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub struct TimerEntry {
    pub id: u64,
    pub label: String,
    pub cancel: Arc<AtomicBool>,
}

pub static ACTIVE_TIMERS: Lazy<Mutex<Vec<TimerEntry>>> = Lazy::new(|| Mutex::new(Vec::new()));

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

pub fn active_timers() -> Vec<(u64, String)> {
    ACTIVE_TIMERS.lock().unwrap().iter().map(|t| (t.id, t.label.clone())).collect()
}

pub fn cancel_timer(id: u64) {
    let mut timers = ACTIVE_TIMERS.lock().unwrap();
    if let Some(pos) = timers.iter().position(|t| t.id == id) {
        let entry = timers.remove(pos);
        entry.cancel.store(true, Ordering::SeqCst);
    }
}

fn notify(msg: &str) {
    #[allow(unused)]
    {
        let _ = notify_rust::Notification::new().summary("Multi Launcher").body(msg).show();
    }
}

pub fn start_timer(duration: Duration) {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    let label = format!("Timer {:?}", duration);
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.push(TimerEntry { id, label: label.clone(), cancel: cancel.clone() });
    }
    thread::spawn(move || {
        let start = Instant::now();
        loop {
            let remaining = duration.saturating_sub(start.elapsed());
            if remaining.is_zero() { break; }
            if cancel.load(Ordering::SeqCst) { return; }
            thread::sleep(Duration::from_millis(100));
        }
        if !cancel.load(Ordering::SeqCst) {
            notify(&format!("Timer finished: {}", label));
        }
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        if let Some(pos) = list.iter().position(|t| t.id == id) { list.remove(pos); }
    });
}

pub fn start_alarm(hour: u32, minute: u32) {
    use chrono::{Timelike, Local, Duration as ChronoDuration};
    let now = Local::now();
    let mut target = now.date_naive().and_hms_opt(hour, minute, 0).unwrap();
    if target <= now.naive_local() {
        target += ChronoDuration::days(1);
    }
    let duration = (target - now.naive_local()).to_std().unwrap_or(Duration::from_secs(0));
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    let label = format!("Alarm {:02}:{:02}", hour, minute);
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.push(TimerEntry { id, label: label.clone(), cancel: cancel.clone() });
    }
    thread::spawn(move || {
        let start = Instant::now();
        loop {
            let remaining = duration.saturating_sub(start.elapsed());
            if remaining.is_zero() { break; }
            if cancel.load(Ordering::SeqCst) { return; }
            thread::sleep(Duration::from_millis(100));
        }
        if !cancel.load(Ordering::SeqCst) {
            notify(&format!("Alarm triggered: {}", label));
        }
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        if let Some(pos) = list.iter().position(|t| t.id == id) { list.remove(pos); }
    });
}

pub struct TimerPlugin;

impl Plugin for TimerPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.starts_with("timer cancel") {
            return active_timers()
                .into_iter()
                .map(|(id, label)| Action {
                    label: format!("Cancel {label}"),
                    desc: "Timer".into(),
                    action: format!("timer:cancel:{id}"),
                    args: None,
                })
                .collect();
        }
        if let Some(arg) = query.strip_prefix("timer ") {
            let arg = arg.trim();
            if let Some(_) = parse_duration(arg) {
                return vec![Action {
                    label: format!("Start timer {arg}"),
                    desc: "Timer".into(),
                    action: format!("timer:start:{arg}"),
                    args: None,
                }];
            }
        }
        if let Some(arg) = query.strip_prefix("alarm ") {
            let arg = arg.trim();
            if parse_hhmm(arg).is_some() {
                return vec![Action {
                    label: format!("Set alarm {arg}"),
                    desc: "Timer".into(),
                    action: format!("alarm:set:{arg}"),
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str { "timer" }

    fn description(&self) -> &str { "Create timers and alarms (prefix: `timer` / `alarm`)" }

    fn capabilities(&self) -> &[&str] { &["search"] }
}

