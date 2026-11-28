//! Integer picker rules for the stock Timer and Alarm text fields.
use multi_launcher::plugins::timer::{self, AlarmDateSpec, AlarmSpec};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Parts { pub hours: u64, pub minutes: u32, pub seconds: u32 }

impl Parts {
    pub fn observe_timer(&mut self, text: &str) {
        if let Some(duration) = timer::parse_duration(text) {
            let total = duration.as_secs();
            self.hours = total / 3600;
            self.minutes = ((total % 3600) / 60) as u32;
            self.seconds = (total % 60) as u32;
        }
    }
    pub fn observe_alarm(&mut self, text: &str) {
        if let Ok(spec) = timer::parse_alarm_spec(text) {
            self.hours = u64::from(spec.hour);
            self.minutes = spec.minute;
        }
    }
}

pub(super) fn parse_parts(hours: &str, minutes: &str, seconds: Option<&str>, alarm: bool) -> Option<Parts> {
    fn integer(text: &str) -> Option<u64> {
        (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())).then(|| text.parse().ok()).flatten()
    }
    let hours = integer(hours)?;
    let minutes = integer(minutes)?;
    let seconds = seconds.map_or(Some(0), integer)?;
    if (alarm && hours > 23) || minutes > 59 || seconds > 59 { return None; }
    Some(Parts { hours, minutes: minutes as u32, seconds: seconds as u32 })
}

pub(super) fn timer_text(parts: Parts) -> String {
    format!("{}:{:02}:{:02}", parts.hours, parts.minutes, parts.seconds)
}

pub(super) fn fields_changed(projected: &[String; 3], current: [&str; 3], alarm: bool) -> bool {
    current[0] != projected[0] || current[1] != projected[1] || (!alarm && current[2] != projected[2])
}

pub(super) fn alarm_text(parts: Parts, current: &str) -> Option<String> {
    let date = if current.trim().is_empty() { AlarmDateSpec::NextOccurrence }
        else { timer::parse_alarm_spec(current).ok()?.date };
    let time = format!("{:02}:{:02}", parts.hours, parts.minutes);
    Some(match date {
        AlarmDateSpec::NextOccurrence => time,
        AlarmDateSpec::DayOffset(days) => format!("{days}d {time}"),
        AlarmDateSpec::ExplicitDate(date) => format!("{} {time}", date.format("%Y-%m-%d")),
    })
}

pub(super) fn alarm_preview<Tz, Map>(text: &str, now: chrono::DateTime<Tz>, map: Map) -> String
where Tz: chrono::TimeZone, Tz::Offset: std::fmt::Display,
    Map: FnMut(chrono::NaiveDateTime) -> chrono::LocalResult<chrono::DateTime<Tz>> {
    if text.trim().is_empty() { return "Choose or enter an alarm time to preview it.".into(); }
    let Ok(spec): Result<AlarmSpec, _> = timer::parse_alarm_spec(text) else { return "Enter a valid alarm date and time.".into(); };
    match timer::resolve_alarm_spec_at(spec, now, map) {
        Ok(target) => format!("Next alarm: {}", target.format("%Y-%m-%d %H:%M:%S %:z")),
        Err(error) => format!("Alarm preview unavailable: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn typed_timer_and_alarm_preserve_last_valid_picker_parts() {
        let mut parts = Parts::default();
        parts.observe_timer("1:02:03");
        assert_eq!(parts, Parts { hours: 1, minutes: 2, seconds: 3 });
        parts.observe_timer("1:");
        assert_eq!(parts, Parts { hours: 1, minutes: 2, seconds: 3 });
        parts.observe_alarm("2d 19:23");
        parts.observe_alarm("2d 19:");
        assert_eq!((parts.hours, parts.minutes), (19, 23));
    }
    #[test]
    fn integer_picker_rejects_fractions_and_preserves_alarm_dates() {
        assert!(parse_parts("1.5", "2", Some("3"), false).is_none());
        assert!(parse_parts("24", "0", None, true).is_none());
        assert!(parse_parts("1", "60", Some("0"), false).is_none());
        let parts = parse_parts(&u64::MAX.to_string(), "59", Some("1"), false).unwrap();
        assert_eq!(timer_text(parts), format!("{}:59:01", u64::MAX));
        let alarm = parse_parts("7", "4", None, true).unwrap();
        assert_eq!(alarm_text(alarm, "2d 19:23").as_deref(), Some("2d 07:04"));
        assert_eq!(alarm_text(alarm, "2026-12-01 19:23").as_deref(), Some("2026-12-01 07:04"));
        assert_eq!(alarm_text(alarm, "").as_deref(), Some("07:04"));
        assert!(alarm_text(alarm, "1d 19:").is_none());
    }
    #[test]
    fn pending_integer_edits_are_compared_to_the_last_projection() {
        let projected = ["2".to_owned(), "03".to_owned(), "04".to_owned()];
        assert!(!fields_changed(&projected, ["2", "03", "04"], false));
        assert!(fields_changed(&projected, ["2", "3", "04"], false));
        assert!(fields_changed(&projected, ["2", "", "04"], false));
        assert!(!fields_changed(&projected, ["2", "03", "different"], true));
    }
    #[test]
    fn alarm_preview_uses_supplied_clock_and_real_resolver() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 11, 28, 12, 0, 0).unwrap();
        assert_eq!(alarm_preview("13:05", now, |time| chrono::Utc.from_local_datetime(&time)), "Next alarm: 2026-11-28 13:05:00 +00:00");
        assert_eq!(alarm_preview("11:59", now, |time| chrono::Utc.from_local_datetime(&time)), "Next alarm: 2026-11-29 11:59:00 +00:00");
        assert_eq!(alarm_preview("broken", now, |time| chrono::Utc.from_local_datetime(&time)), "Enter a valid alarm date and time.");
        assert!(alarm_preview("2026-11-27 13:05", now, |time| chrono::Utc.from_local_datetime(&time)).starts_with("Alarm preview unavailable:"));
    }
}
