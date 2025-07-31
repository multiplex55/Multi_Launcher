use crate::actions::Action;
use crate::plugin::Plugin;
use eframe::egui;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static PRECISION: AtomicU32 = AtomicU32::new(2);
static REFRESH_RATE_MS: AtomicU32 = AtomicU32::new(1_000);

#[derive(Clone)]
pub struct StopwatchEntry {
    pub id: u64,
    pub label: String,
    pub start: Instant,
    pub elapsed: Duration,
    pub paused: bool,
    pub generation: u64,
}

pub static STOPWATCHES: Lazy<Mutex<HashMap<u64, StopwatchEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn precision() -> u32 {
    PRECISION.load(Ordering::Relaxed)
}

pub fn set_precision(p: u32) {
    PRECISION.store(p, Ordering::Relaxed);
}

fn refresh_rate_ms() -> u32 {
    REFRESH_RATE_MS.load(Ordering::Relaxed)
}

pub fn refresh_rate() -> f32 {
    refresh_rate_ms() as f32 / 1000.0
}

pub fn set_refresh_rate(secs: f32) {
    let ms = (secs * 1000.0).clamp(0.0, 5_000.0) as u32;
    REFRESH_RATE_MS.store(ms, Ordering::Relaxed);
}

pub fn start_stopwatch_named(name: Option<String>) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let label = name.unwrap_or_else(|| format!("Stopwatch {id}"));
    let entry = StopwatchEntry {
        id,
        label,
        start: Instant::now(),
        elapsed: Duration::ZERO,
        paused: false,
        generation: 0,
    };
    if let Ok(mut guard) = STOPWATCHES.lock() {
        guard.insert(id, entry);
    }
    id
}

pub fn pause_stopwatch(id: u64) {
    if let Ok(mut guard) = STOPWATCHES.lock() {
        if let Some(sw) = guard.get_mut(&id) {
            if !sw.paused {
                let now = Instant::now();
                sw.elapsed += now.saturating_duration_since(sw.start);
                sw.paused = true;
                sw.generation += 1;
            }
        }
    }
}

pub fn resume_stopwatch(id: u64) {
    if let Ok(mut guard) = STOPWATCHES.lock() {
        if let Some(sw) = guard.get_mut(&id) {
            if sw.paused {
                sw.start = Instant::now();
                sw.paused = false;
                sw.generation += 1;
            }
        }
    }
}

pub fn stop_stopwatch(id: u64) {
    if let Ok(mut guard) = STOPWATCHES.lock() {
        guard.remove(&id);
    }
}

pub fn running_stopwatches() -> Vec<(u64, String, Duration)> {
    if let Ok(guard) = STOPWATCHES.lock() {
        guard
            .values()
            .filter(|s| !s.paused)
            .map(|s| {
                let elapsed = s.elapsed + Instant::now().saturating_duration_since(s.start);
                (s.id, s.label.clone(), elapsed)
            })
            .collect()
    } else {
        Vec::new()
    }
}

pub fn paused_stopwatches() -> Vec<(u64, String, Duration)> {
    if let Ok(guard) = STOPWATCHES.lock() {
        guard
            .values()
            .filter(|s| s.paused)
            .map(|s| (s.id, s.label.clone(), s.elapsed))
            .collect()
    } else {
        Vec::new()
    }
}

pub fn all_stopwatches() -> Vec<(u64, String, Duration, bool)> {
    if let Ok(guard) = STOPWATCHES.lock() {
        guard
            .values()
            .map(|s| {
                let running = !s.paused;
                let mut elapsed = s.elapsed;
                if running {
                    elapsed += Instant::now().saturating_duration_since(s.start);
                }
                (s.id, s.label.clone(), elapsed, running)
            })
            .collect()
    } else {
        Vec::new()
    }
}

pub fn format_duration(dur: Duration) -> String {
    let p = precision().min(9);
    let hours = dur.as_secs() / 3600;
    let minutes = (dur.as_secs() % 3600) / 60;
    let seconds = dur.as_secs() % 60;
    let mut s = format!("{hours:02}:{minutes:02}:{seconds:02}");
    if p > 0 {
        let mut frac = dur.subsec_nanos();
        if p < 9 {
            frac /= 10u32.pow(9 - p);
        }
        s.push('.');
        s.push_str(&format!("{frac:0width$}", width = p as usize));
    }
    s
}

pub fn format_elapsed(id: u64) -> Option<String> {
    if let Ok(guard) = STOPWATCHES.lock() {
        guard.get(&id).map(|s| {
            let mut elapsed = s.elapsed;
            if !s.paused {
                elapsed += Instant::now().saturating_duration_since(s.start);
            }
            format_duration(elapsed)
        })
    } else {
        None
    }
}

#[derive(Serialize, Deserialize)]
pub struct StopwatchPluginSettings {
    pub precision: u32,
    pub refresh_rate: f32,
}

impl Default for StopwatchPluginSettings {
    fn default() -> Self {
        Self {
            precision: 2,
            refresh_rate: 1.0,
        }
    }
}

pub struct StopwatchPlugin {
    precision: u32,
    refresh_rate: f32,
}

impl Default for StopwatchPlugin {
    fn default() -> Self {
        let cfg = StopwatchPluginSettings::default();
        set_precision(cfg.precision);
        set_refresh_rate(cfg.refresh_rate);
        Self {
            precision: cfg.precision,
            refresh_rate: cfg.refresh_rate,
        }
    }
}

impl Plugin for StopwatchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let Some(rest) = crate::common::strip_prefix_ci(trimmed, "sw") else {
            return Vec::new();
        };
        let rest = rest.trim();
        if let Some(arg) = crate::common::strip_prefix_ci(rest, "start") {
            let name = arg.trim();
            let action = format!("stopwatch:start:{name}");
            let label = if name.is_empty() {
                "Start stopwatch".to_string()
            } else {
                format!("Start stopwatch {name}")
            };
            return vec![Action {
                label,
                desc: "Stopwatch".into(),
                action,
                args: None,
            }];
        }
        if crate::common::strip_prefix_ci(rest, "list").is_some() || rest.is_empty() {
            return all_stopwatches()
                .into_iter()
                .map(|(id, label, elapsed, running)| {
                    let time = format_duration(elapsed);
                    let label = if running {
                        format!("{label} ({time})")
                    } else {
                        format!("{label} ({time}, paused)")
                    };
                    Action {
                        label,
                        desc: "Stopwatch".into(),
                        action: format!("stopwatch:show:{id}"),
                        args: None,
                    }
                })
                .collect();
        }
        if let Some(arg) = crate::common::strip_prefix_ci(rest, "pause") {
            let tail = arg.trim();
            if tail.is_empty() {
                return running_stopwatches()
                    .into_iter()
                    .map(|(id, label, _)| Action {
                        label: format!("Pause {label}"),
                        desc: "Stopwatch".into(),
                        action: format!("stopwatch:pause:{id}"),
                        args: None,
                    })
                    .collect();
            } else if let Ok(id) = tail.parse::<u64>() {
                return vec![Action {
                    label: format!("Pause stopwatch {id}"),
                    desc: "Stopwatch".into(),
                    action: format!("stopwatch:pause:{id}"),
                    args: None,
                }];
            }
        }
        if let Some(arg) = crate::common::strip_prefix_ci(rest, "resume") {
            let tail = arg.trim();
            if tail.is_empty() {
                return paused_stopwatches()
                    .into_iter()
                    .map(|(id, label, _)| Action {
                        label: format!("Resume {label}"),
                        desc: "Stopwatch".into(),
                        action: format!("stopwatch:resume:{id}"),
                        args: None,
                    })
                    .collect();
            } else if let Ok(id) = tail.parse::<u64>() {
                return vec![Action {
                    label: format!("Resume stopwatch {id}"),
                    desc: "Stopwatch".into(),
                    action: format!("stopwatch:resume:{id}"),
                    args: None,
                }];
            }
        }
        if let Some(arg) = crate::common::strip_prefix_ci(rest, "stop") {
            let tail = arg.trim();
            if tail.is_empty() {
                return all_stopwatches()
                    .into_iter()
                    .map(|(id, label, _, _)| Action {
                        label: format!("Stop {label}"),
                        desc: "Stopwatch".into(),
                        action: format!("stopwatch:stop:{id}"),
                        args: None,
                    })
                    .collect();
            } else if let Ok(id) = tail.parse::<u64>() {
                return vec![Action {
                    label: format!("Stop stopwatch {id}"),
                    desc: "Stopwatch".into(),
                    action: format!("stopwatch:stop:{id}"),
                    args: None,
                }];
            }
        }
        if let Some(arg) = crate::common::strip_prefix_ci(rest, "show") {
            let tail = arg.trim();
            if tail.is_empty() {
                return all_stopwatches()
                    .into_iter()
                    .map(|(id, label, elapsed, running)| {
                        let time = format_duration(elapsed);
                        let label = if running {
                            format!("{label} ({time})")
                        } else {
                            format!("{label} ({time}, paused)")
                        };
                        Action {
                            label,
                            desc: "Stopwatch".into(),
                            action: format!("stopwatch:show:{id}"),
                            args: None,
                        }
                    })
                    .collect();
            } else if let Ok(id) = tail.parse::<u64>() {
                if let Some(time) = format_elapsed(id) {
                    return vec![Action {
                        label: format!("Stopwatch {id}: {time}"),
                        desc: "Stopwatch".into(),
                        action: format!("stopwatch:show:{id}"),
                        args: None,
                    }];
                }
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "stopwatch"
    }

    fn description(&self) -> &str {
        "Simple stopwatches (prefix: `sw`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "sw start".into(),
                desc: "Stopwatch".into(),
                action: "query:sw start ".into(),
                args: None,
            },
            Action {
                label: "sw list".into(),
                desc: "Stopwatch".into(),
                action: "query:sw list".into(),
                args: None,
            },
            Action {
                label: "sw pause".into(),
                desc: "Stopwatch".into(),
                action: "query:sw pause".into(),
                args: None,
            },
            Action {
                label: "sw resume".into(),
                desc: "Stopwatch".into(),
                action: "query:sw resume".into(),
                args: None,
            },
            Action {
                label: "sw stop".into(),
                desc: "Stopwatch".into(),
                action: "query:sw stop".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(StopwatchPluginSettings {
            precision: self.precision,
            refresh_rate: self.refresh_rate,
        })
        .ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<StopwatchPluginSettings>(value.clone()) {
            self.precision = cfg.precision;
            self.refresh_rate = cfg.refresh_rate;
            set_precision(cfg.precision);
            set_refresh_rate(cfg.refresh_rate);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: StopwatchPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Precision");
            ui.add(egui::Slider::new(&mut cfg.precision, 0..=9));
        });
        ui.horizontal(|ui| {
            ui.label("Refresh (s)");
            ui.add(
                egui::DragValue::new(&mut cfg.refresh_rate)
                    .clamp_range(0.0..=5.0)
                    .speed(0.1),
            );
        });
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!("failed to serialize stopwatch settings: {e}"),
        }
        self.apply_settings(value);
    }
}
