use crate::actions::Action;
use crate::plugin::Plugin;
use chrono::{Local, NaiveDateTime, NaiveTime, TimeZone, Timelike};

pub struct TimestampPlugin;

impl TimestampPlugin {
    fn ms_to_string(ms: i64) -> Option<String> {
        if ms < 0 {
            return None;
        }
        if ms < 86_400_000 {
            let secs = ms / 1000;
            let nanos = (ms % 1000) as u32 * 1_000_000;
            if let Some(t) = NaiveTime::from_num_seconds_from_midnight_opt(secs as u32, nanos) {
                return Some(t.format("%H:%M:%S").to_string());
            }
        }
        let secs = ms / 1000;
        let nanos = (ms % 1000) as u32 * 1_000_000;
        Local
            .timestamp_opt(secs, nanos)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
    }

    fn string_to_ms(s: &str) -> Option<i64> {
        for fmt in ["%H:%M:%S%.f", "%H:%M:%S", "%H:%M"] {
            if let Ok(t) = NaiveTime::parse_from_str(s, fmt) {
                return Some(
                    (t.num_seconds_from_midnight() as i64) * 1000
                        + (t.nanosecond() / 1_000_000) as i64,
                );
            }
        }
        for fmt in [
            "%Y-%m-%d %H:%M:%S%.f",
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d %H:%M",
        ] {
            if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
                let midnight = dt.date().and_hms_opt(0, 0, 0)?;
                return Some((dt - midnight).num_milliseconds());
            }
        }
        None
    }
}

impl Plugin for TimestampPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "ts ";
        const MS_PREFIX: &str = "tsm ";
        let trimmed = query.trim_start();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, MS_PREFIX) {
            let arg = rest.trim();
            if arg.is_empty() {
                return Vec::new();
            }
            if let Ok(num) = arg.parse::<i64>() {
                if let Some(out) = Self::ms_to_string(num) {
                    return vec![Action {
                        label: out.clone(),
                        desc: "Midnight TS".into(),
                        action: format!("clipboard:{out}"),
                        args: None,
                        preview_text: None,
                        risk_level: None,
                        icon: None,
                    }];
                }
            } else if let Some(ms) = Self::string_to_ms(arg) {
                let out = ms.to_string();
                return vec![Action {
                    label: out.clone(),
                    desc: "Midnight TS".into(),
                    action: format!("clipboard:{out}"),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                }];
            }
        } else if let Some(rest) = crate::common::strip_prefix_ci(trimmed, PREFIX) {
            let arg = rest.trim();
            if arg.is_empty() {
                return Vec::new();
            }
            if let Ok(num) = arg.parse::<i64>() {
                let ts_sec = if num.abs() > 1_000_000_000_000 {
                    num / 1000
                } else {
                    num
                };
                let dt = Local
                    .timestamp_opt(ts_sec, 0)
                    .single()
                    .or_else(|| Local.timestamp_opt(0, 0).single());
                if let Some(dt) = dt {
                    let out = dt.format("%Y-%m-%d %H:%M:%S").to_string();
                    return vec![Action {
                        label: out.clone(),
                        desc: "Timestamp".into(),
                        action: format!("clipboard:{out}"),
                        args: None,
                        preview_text: None,
                        risk_level: None,
                        icon: None,
                    }];
                }
            } else {
                let parsed = NaiveDateTime::parse_from_str(arg, "%Y-%m-%d %H:%M:%S")
                    .or_else(|_| NaiveDateTime::parse_from_str(arg, "%Y-%m-%d %H:%M"));
                if let Ok(naive) = parsed {
                    if let Some(dt) = Local.from_local_datetime(&naive).single() {
                        let ts = dt.timestamp().to_string();
                        return vec![Action {
                            label: ts.clone(),
                            desc: "Timestamp".into(),
                            action: format!("clipboard:{ts}"),
                            args: None,
                            preview_text: None,
                            risk_level: None,
                            icon: None,
                        }];
                    }
                }
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "timestamp"
    }

    fn description(&self) -> &str {
        "Convert timestamps (prefix: `ts`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "ts <value>".into(),
                desc: "Timestamp".into(),
                action: "query:ts ".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "tsm <value>".into(),
                desc: "Midnight TS".into(),
                action: "query:tsm ".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
        ]
    }
}
