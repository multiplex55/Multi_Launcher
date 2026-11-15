//! Authored metadata filters and their immutable submission-time predicates.
use super::model::{FileKind, SearchKind};
use chrono::{DateTime, Days, LocalResult, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateMode {
    #[default]
    Any,
    Today,
    Yesterday,
    Last24Hours,
    Last7Days,
    Last30Days,
    Custom,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatePreference {
    pub mode: DateMode,
    pub start: String,
    pub end: String,
}
impl DatePreference {
    pub fn validate_draft(&self) -> Result<(), String> {
        if self.mode == DateMode::Custom {
            let start = parse_date(&self.start)?;
            let end = parse_date(&self.end)?;
            if start > end {
                return Err("Start date must not be after end date.".into());
            }
            end.checked_add_days(Days::new(1))
                .ok_or("End date is outside the supported range.")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeMode {
    #[default]
    Any,
    Empty,
    SmallerThan,
    LargerThan,
    Between,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeUnit {
    #[default]
    B,
    KB,
    MB,
    GB,
    KiB,
    MiB,
    GiB,
}
impl SizeUnit {
    pub fn multiplier(self) -> u64 {
        match self {
            Self::B => 1,
            Self::KB => 1_000,
            Self::MB => 1_000_000,
            Self::GB => 1_000_000_000,
            Self::KiB => 1024,
            Self::MiB => 1024 * 1024,
            Self::GiB => 1024 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SizePreference {
    pub mode: SizeMode,
    pub lower: String,
    pub upper: String,
    pub unit: SizeUnit,
}
impl SizePreference {
    pub fn resolve_draft(&self) -> Result<Option<SizePredicate>, String> {
        Ok(match self.mode {
            SizeMode::Any => None,
            SizeMode::Empty => Some(SizePredicate::Empty),
            SizeMode::SmallerThan => Some(SizePredicate::SmallerThan(parse_bytes(
                &self.upper,
                self.unit,
            )?)),
            SizeMode::LargerThan => Some(SizePredicate::LargerThan(parse_bytes(
                &self.lower,
                self.unit,
            )?)),
            SizeMode::Between => {
                let min = parse_bytes(&self.lower, self.unit)?;
                let max = parse_bytes(&self.upper, self.unit)?;
                if min > max {
                    return Err("Minimum file size must not exceed maximum file size.".into());
                }
                Some(SizePredicate::Between(min, max))
            }
        })
    }
    pub fn content_warning(&self, cap: u64) -> Result<Option<String>, String> {
        content_warning(self.resolve_draft()?.as_ref(), cap)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetadataPreferences {
    pub date: DatePreference,
    pub size: SizePreference,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateWindow {
    pub start: SystemTime,
    pub end: SystemTime,
    pub end_inclusive: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SizePredicate {
    Empty,
    SmallerThan(u64),
    LargerThan(u64),
    Between(u64, u64),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedMetadata {
    pub date: Option<DateWindow>,
    pub size: Option<SizePredicate>,
    pub warning: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingMetadata {
    Modified,
    Size,
}

impl ResolvedMetadata {
    pub fn active(&self) -> bool {
        self.date.is_some() || self.size.is_some()
    }
    pub fn matches(
        &self,
        kind: FileKind,
        modified: Option<SystemTime>,
        size: Option<u64>,
    ) -> Result<bool, MissingMetadata> {
        if self.size.is_some() && kind != FileKind::File {
            return Ok(false);
        }
        if let Some(date) = &self.date {
            let value = modified.ok_or(MissingMetadata::Modified)?;
            if value < date.start
                || if date.end_inclusive {
                    value > date.end
                } else {
                    value >= date.end
                }
            {
                return Ok(false);
            }
        }
        if let Some(predicate) = &self.size {
            let value = size.ok_or(MissingMetadata::Size)?;
            let accepted = match *predicate {
                SizePredicate::Empty => value == 0,
                SizePredicate::SmallerThan(limit) => value < limit,
                SizePredicate::LargerThan(limit) => value > limit,
                SizePredicate::Between(min, max) => value >= min && value <= max,
            };
            if !accepted {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn for_content(&self, cap: u64) -> Result<Self, String> {
        let mut result = self.clone();
        result.warning = content_warning(result.size.as_ref(), cap)?;
        Ok(result)
    }
}

/// Evaluate a filesystem entry with the same frozen predicate on every backend.
pub(crate) fn metadata_matches(
    filters: &ResolvedMetadata,
    metadata: Option<&std::fs::Metadata>,
) -> Result<bool, MissingMetadata> {
    if !filters.active() {
        return Ok(true);
    }
    let metadata = metadata.ok_or(if filters.date.is_some() {
        MissingMetadata::Modified
    } else {
        MissingMetadata::Size
    })?;
    let kind = if metadata.is_file() {
        FileKind::File
    } else if metadata.is_dir() {
        FileKind::Directory
    } else {
        FileKind::Other
    };
    filters.matches(
        kind,
        metadata.modified().ok(),
        metadata.is_file().then(|| metadata.len()),
    )
}

#[cfg(test)]
mod backend_metadata_tests {
    use super::*;
    #[test]
    fn missing_metadata_is_reported_only_when_required() {
        assert_eq!(
            metadata_matches(&ResolvedMetadata::default(), None),
            Ok(true)
        );
        let size = ResolvedMetadata {
            size: Some(SizePredicate::Empty),
            ..Default::default()
        };
        assert_eq!(metadata_matches(&size, None), Err(MissingMetadata::Size));
        let date = ResolvedMetadata {
            date: Some(DateWindow {
                start: SystemTime::UNIX_EPOCH,
                end: SystemTime::UNIX_EPOCH,
                end_inclusive: true,
            }),
            ..Default::default()
        };
        assert_eq!(
            metadata_matches(&date, None),
            Err(MissingMetadata::Modified)
        );
    }
}

/// Convert a decimal amount to whole bytes without floating-point rounding.
pub fn parse_bytes(text: &str, unit: SizeUnit) -> Result<u64, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 64 || text.starts_with('-') {
        return Err("Enter a nonnegative decimal file size.".into());
    }
    let text = text.strip_prefix('+').unwrap_or(text);
    let mut parts = text.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    if parts.next().is_some()
        || (whole.is_empty() && fraction.is_none_or(str::is_empty))
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_some_and(|digits| !digits.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("Enter a nonnegative decimal file size.".into());
    }
    let whole = if whole.is_empty() {
        0
    } else {
        whole
            .parse::<u128>()
            .map_err(|_| "File size is too large.")?
    };
    let fraction = fraction.unwrap_or("").trim_end_matches('0');
    if fraction.len() > 30 {
        return Err("File size must resolve to a whole number of bytes.".into());
    }
    let scale = 10u128
        .checked_pow(fraction.len() as u32)
        .ok_or("File size precision is too large.")?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u128>()
            .map_err(|_| "File size precision is too large.")?
    };
    let multiplier = u128::from(unit.multiplier());
    let mut a = scale;
    let mut b = multiplier;
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    let denominator = scale / a;
    if fraction % denominator != 0 {
        return Err("File size must resolve to a whole number of bytes.".into());
    }
    let fractional_bytes = (fraction / denominator)
        .checked_mul(multiplier / a)
        .ok_or("File size is too large.")?;
    let bytes = whole
        .checked_mul(multiplier)
        .and_then(|value| value.checked_add(fractional_bytes))
        .ok_or("File size is too large.")?;
    u64::try_from(bytes).map_err(|_| "File size is too large.".into())
}

fn system_time(value: DateTime<Utc>) -> Result<SystemTime, String> {
    let seconds = value.timestamp();
    let nanos = value.timestamp_subsec_nanos();
    let time = if seconds >= 0 {
        SystemTime::UNIX_EPOCH.checked_add(Duration::new(seconds as u64, nanos))
    } else {
        SystemTime::UNIX_EPOCH
            .checked_sub(Duration::from_secs(seconds.unsigned_abs()))
            .and_then(|time| time.checked_add(Duration::from_nanos(u64::from(nanos))))
    };
    time.ok_or_else(|| "Date is outside the supported system time range.".into())
}

fn parse_date(text: &str) -> Result<NaiveDate, String> {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err("Enter dates as YYYY-MM-DD.".into());
    }
    let date =
        NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| "Enter dates as YYYY-MM-DD.")?;
    if date.format("%Y-%m-%d").to_string() != text {
        return Err("Enter dates as YYYY-MM-DD.".into());
    }
    Ok(date)
}

fn midnight(date: NaiveDate, zone: Tz) -> Result<SystemTime, String> {
    let local = date
        .and_hms_opt(0, 0, 0)
        .ok_or("Date is outside the supported range.")?;
    let resolved = match zone.from_local_datetime(&local) {
        LocalResult::Single(value) => value,
        LocalResult::Ambiguous(_, _) => {
            return Err("Local midnight is ambiguous in the captured timezone.".into());
        }
        LocalResult::None => {
            return Err("Local midnight does not exist in the captured timezone.".into());
        }
    };
    system_time(resolved.with_timezone(&Utc))
}

fn date_window(
    preference: &DatePreference,
    now: DateTime<Utc>,
    zone: Option<Tz>,
) -> Result<Option<DateWindow>, String> {
    let rolling_days = match preference.mode {
        DateMode::Last24Hours => Some(1),
        DateMode::Last7Days => Some(7),
        DateMode::Last30Days => Some(30),
        _ => None,
    };
    if let Some(days) = rolling_days {
        let start = now
            .checked_sub_signed(chrono::Duration::days(days))
            .ok_or("Rolling date is out of range.")?;
        return Ok(Some(DateWindow {
            start: system_time(start)?,
            end: system_time(now)?,
            end_inclusive: true,
        }));
    }
    if preference.mode == DateMode::Any {
        return Ok(None);
    }
    let zone = zone.ok_or("The local timezone could not be captured.")?;
    let today = now.with_timezone(&zone).date_naive();
    let (start, end) = match preference.mode {
        DateMode::Today => (today, today),
        DateMode::Yesterday => {
            let date = today
                .checked_sub_days(Days::new(1))
                .ok_or("Date is outside the supported range.")?;
            (date, date)
        }
        DateMode::Custom => (parse_date(&preference.start)?, parse_date(&preference.end)?),
        _ => return Err("Invalid date filter.".into()),
    };
    if start > end {
        return Err("Start date must not be after end date.".into());
    }
    let next = end
        .checked_add_days(Days::new(1))
        .ok_or("End date is outside the supported range.")?;
    Ok(Some(DateWindow {
        start: midnight(start, zone)?,
        end: midnight(next, zone)?,
        end_inclusive: false,
    }))
}

impl MetadataPreferences {
    pub fn semantic(&self) -> Self {
        let mut result = self.clone();
        if result.date.mode != DateMode::Custom {
            result.date.start.clear();
            result.date.end.clear();
        }
        match result.size.mode {
            SizeMode::Any | SizeMode::Empty => {
                result.size.lower.clear();
                result.size.upper.clear();
                result.size.unit = SizeUnit::B;
            }
            SizeMode::SmallerThan => result.size.lower.clear(),
            SizeMode::LargerThan => result.size.upper.clear(),
            SizeMode::Between => {}
        }
        let interval = match result.size.mode {
            SizeMode::Any => None,
            SizeMode::Empty => Some(Some((0, 0))),
            SizeMode::SmallerThan => parse_bytes(&result.size.upper, result.size.unit)
                .ok()
                .map(|value| value.checked_sub(1).map(|max| (0, max))),
            SizeMode::LargerThan => parse_bytes(&result.size.lower, result.size.unit)
                .ok()
                .map(|value| value.checked_add(1).map(|min| (min, u64::MAX))),
            SizeMode::Between => parse_bytes(&result.size.lower, result.size.unit)
                .ok()
                .zip(parse_bytes(&result.size.upper, result.size.unit).ok())
                .and_then(|(min, max)| (min <= max).then_some(Some((min, max)))),
        };
        if let Some(interval) = interval {
            result.size = match interval {
                Some((min, max)) => SizePreference {
                    mode: SizeMode::Between,
                    lower: min.to_string(),
                    upper: max.to_string(),
                    unit: SizeUnit::B,
                },
                None => SizePreference {
                    mode: SizeMode::SmallerThan,
                    lower: String::new(),
                    upper: "0".into(),
                    unit: SizeUnit::B,
                },
            };
        }
        result
    }

    pub fn resolve_at(
        &self,
        now: DateTime<Utc>,
        zone: Option<Tz>,
        kind: SearchKind,
        content_cap: u64,
    ) -> Result<ResolvedMetadata, String> {
        let date = date_window(&self.date, now, zone)?;
        let size = self.size.resolve_draft()?;
        let warning = if kind == SearchKind::Content {
            content_warning(size.as_ref(), content_cap)?
        } else {
            None
        };
        Ok(ResolvedMetadata {
            date,
            size,
            warning,
        })
    }

    /// Capture one instant and, only for local calendar filters, one zone name.
    pub fn capture(&self, kind: SearchKind, content_cap: u64) -> Result<ResolvedMetadata, String> {
        let now = Utc::now();
        let zone = if matches!(
            self.date.mode,
            DateMode::Today | DateMode::Yesterday | DateMode::Custom
        ) {
            let name = iana_time_zone::get_timezone()
                .map_err(|_| "The local timezone could not be captured.")?;
            Some(
                name.parse::<Tz>()
                    .map_err(|_| "The captured local timezone is unsupported.")?,
            )
        } else {
            None
        };
        self.resolve_at(now, zone, kind, content_cap)
    }
}

fn content_warning(
    size: Option<&SizePredicate>,
    content_cap: u64,
) -> Result<Option<String>, String> {
    let Some(predicate) = size else {
        return Ok(None);
    };
    let bounds = match *predicate {
        SizePredicate::Empty => Some((0, 0)),
        SizePredicate::SmallerThan(n) => n.checked_sub(1).map(|max| (0, max)),
        SizePredicate::LargerThan(n) => n.checked_add(1).map(|min| (min, u64::MAX)),
        SizePredicate::Between(min, max) => Some((min, max)),
    };
    let (min, max) = bounds.ok_or("This size filter cannot match any file.")?;
    if max == 0 {
        return Err("Empty files cannot contain the requested text.".into());
    }
    if min.max(1) > content_cap {
        return Err("The size filter is entirely above the content-reading size limit.".into());
    }
    Ok((max > content_cap).then(|| format!("Only files up to {content_cap} bytes can be scanned for content; larger matching files are excluded.")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preferences(date: DateMode, size: SizeMode) -> MetadataPreferences {
        MetadataPreferences {
            date: DatePreference {
                mode: date,
                ..Default::default()
            },
            size: SizePreference {
                mode: size,
                ..Default::default()
            },
        }
    }

    #[test]
    fn draft_validation_is_clock_free_and_reports_independent_content_limit() {
        let date = DatePreference {
            mode: DateMode::Custom,
            start: "2026-11-16".into(),
            end: "2026-11-15".into(),
        };
        assert!(date.validate_draft().is_err());
        let mut size = SizePreference {
            mode: SizeMode::Between,
            lower: "1".into(),
            upper: "3".into(),
            unit: SizeUnit::KB,
        };
        assert!(size.resolve_draft().is_ok());
        assert!(size.content_warning(2000).unwrap().is_some());
        size.lower = "4".into();
        assert!(size.resolve_draft().is_err());
    }

    #[test]
    fn decimal_size_boundaries_are_checked_without_float_rounding() {
        assert_eq!(parse_bytes("1.5", SizeUnit::KB).unwrap(), 1500);
        assert_eq!(parse_bytes("1.5", SizeUnit::KiB).unwrap(), 1536);
        assert_eq!(parse_bytes("+.5", SizeUnit::KiB).unwrap(), 512);
        assert_eq!(parse_bytes("1.", SizeUnit::B).unwrap(), 1);
        assert_eq!(
            parse_bytes("17179869183.999999999068677425384521484375", SizeUnit::GiB).unwrap(),
            u64::MAX
        );
        assert_eq!(
            parse_bytes("18446744073709551615.000000000000000000000", SizeUnit::B).unwrap(),
            u64::MAX
        );
        for invalid in ["-1", "0.1", "18446744073709551616", "1e3", "."] {
            assert!(parse_bytes(invalid, SizeUnit::B).is_err(), "{invalid}");
        }
    }

    #[test]
    fn calendar_and_rolling_windows_freeze_boundaries_across_dst() {
        let now = Utc.with_ymd_and_hms(2026, 3, 8, 16, 0, 0).unwrap();
        let zone: Tz = "America/New_York".parse().unwrap();
        let calendar = preferences(DateMode::Today, SizeMode::Any)
            .resolve_at(now, Some(zone), SearchKind::Filename, 1024)
            .unwrap();
        let window = calendar.date.unwrap();
        assert_eq!(
            window.end.duration_since(window.start).unwrap(),
            Duration::from_secs(23 * 3600)
        );
        assert!(!window.end_inclusive);
        let calendar = ResolvedMetadata {
            date: Some(window.clone()),
            ..Default::default()
        };
        assert_eq!(
            calendar.matches(FileKind::Directory, Some(window.start), None),
            Ok(true)
        );
        assert_eq!(
            calendar.matches(FileKind::Directory, Some(window.end), None),
            Ok(false)
        );
        let rolling = preferences(DateMode::Last24Hours, SizeMode::Any)
            .resolve_at(now, Some(zone), SearchKind::Filename, 1024)
            .unwrap();
        let rolling = rolling.date.unwrap();
        assert_eq!(
            rolling.end.duration_since(rolling.start).unwrap(),
            Duration::from_secs(24 * 3600)
        );
        assert!(rolling.end_inclusive);
        let mut invalid = preferences(DateMode::Custom, SizeMode::Any);
        invalid.date.start = "+2026-03-01".into();
        invalid.date.end = "2026-03-02".into();
        assert!(invalid
            .resolve_at(now, Some(zone), SearchKind::Filename, 1024)
            .is_err());
        invalid.date.start = "2026-03-03".into();
        assert!(invalid
            .resolve_at(now, Some(zone), SearchKind::Filename, 1024)
            .is_err());
        let sao_paulo = "America/Sao_Paulo".parse().unwrap();
        invalid.date.start = "2018-11-04".into();
        invalid.date.end = "2018-11-04".into();
        assert!(invalid
            .resolve_at(now, Some(sao_paulo), SearchKind::Filename, 1024)
            .is_err());
    }

    #[test]
    fn metadata_checks_unknowns_directories_and_content_intersections() {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
        let mut prefs = preferences(DateMode::Last7Days, SizeMode::Between);
        prefs.size.lower = "2".into();
        prefs.size.upper = "10".into();
        let resolved = prefs
            .resolve_at(now, None, SearchKind::Filename, 5)
            .unwrap();
        let date = resolved.date.as_ref().unwrap();
        assert_eq!(
            resolved.matches(FileKind::File, Some(date.start), Some(2)),
            Ok(true)
        );
        assert_eq!(
            resolved.matches(FileKind::File, Some(date.end), Some(10)),
            Ok(true)
        );
        assert_eq!(
            resolved.matches(FileKind::File, Some(date.end), Some(11)),
            Ok(false)
        );
        assert_eq!(
            resolved.matches(FileKind::Directory, Some(date.end), Some(2)),
            Ok(false)
        );
        assert_eq!(
            resolved.matches(FileKind::File, None, Some(2)),
            Err(MissingMetadata::Modified)
        );
        assert_eq!(
            resolved.matches(FileKind::File, Some(date.end), None),
            Err(MissingMetadata::Size)
        );
        let smaller = ResolvedMetadata {
            size: Some(SizePredicate::SmallerThan(10)),
            ..Default::default()
        };
        assert_eq!(smaller.matches(FileKind::File, None, Some(9)), Ok(true));
        assert_eq!(smaller.matches(FileKind::File, None, Some(10)), Ok(false));
        let larger = ResolvedMetadata {
            size: Some(SizePredicate::LargerThan(10)),
            ..Default::default()
        };
        assert_eq!(larger.matches(FileKind::File, None, Some(10)), Ok(false));
        assert_eq!(larger.matches(FileKind::File, None, Some(11)), Ok(true));
        assert!(prefs
            .resolve_at(now, None, SearchKind::Content, 5)
            .unwrap()
            .warning
            .is_some());
        prefs.size.lower = "6".into();
        assert!(prefs.resolve_at(now, None, SearchKind::Content, 5).is_err());
        prefs.size.mode = SizeMode::Empty;
        assert!(prefs.resolve_at(now, None, SearchKind::Content, 5).is_err());
        assert_eq!(
            ResolvedMetadata::default().matches(FileKind::Directory, None, None),
            Ok(true)
        );
    }

    #[test]
    fn old_preferences_default_off_and_semantic_keys_ignore_inactive_fields() {
        let old: super::super::settings::FileSearchUiPreferences =
            serde_json::from_str("{}").unwrap();
        assert_eq!(old.metadata, MetadataPreferences::default());
        let mut left = MetadataPreferences::default();
        let right = left.clone();
        left.date.start = "unsubmitted".into();
        left.size.lower = "999".into();
        assert_eq!(left.semantic(), right.semantic());
        let smaller = |upper: &str, unit| MetadataPreferences {
            size: SizePreference {
                mode: SizeMode::SmallerThan,
                upper: upper.into(),
                unit,
                ..Default::default()
            },
            ..Default::default()
        };
        let one_kb = smaller("1", SizeUnit::KB);
        assert_eq!(one_kb.semantic(), smaller("1000", SizeUnit::B).semantic());
        assert_eq!(one_kb.semantic(), smaller("1.0", SizeUnit::KB).semantic());
        let between = MetadataPreferences {
            size: SizePreference {
                mode: SizeMode::Between,
                lower: "0".into(),
                upper: "999".into(),
                unit: SizeUnit::B,
            },
            ..Default::default()
        };
        assert_eq!(one_kb.semantic(), between.semantic());
        let larger = MetadataPreferences {
            size: SizePreference {
                mode: SizeMode::LargerThan,
                lower: "999".into(),
                unit: SizeUnit::B,
                ..Default::default()
            },
            ..Default::default()
        };
        let inclusive = MetadataPreferences {
            size: SizePreference {
                mode: SizeMode::Between,
                lower: "1000".into(),
                upper: u64::MAX.to_string(),
                unit: SizeUnit::B,
            },
            ..Default::default()
        };
        assert_eq!(larger.semantic(), inclusive.semantic());
        let impossible = MetadataPreferences {
            size: SizePreference {
                mode: SizeMode::LargerThan,
                lower: u64::MAX.to_string(),
                unit: SizeUnit::B,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(impossible.semantic(), smaller("0", SizeUnit::B).semantic());
        assert_ne!(one_kb.size.unit, one_kb.semantic().size.unit);
        assert_ne!(
            smaller("bad", SizeUnit::KB).semantic(),
            smaller("bad", SizeUnit::B).semantic()
        );
        assert_ne!(
            MetadataPreferences::default().semantic(),
            MetadataPreferences {
                size: SizePreference {
                    mode: SizeMode::Between,
                    lower: "0".into(),
                    upper: u64::MAX.to_string(),
                    unit: SizeUnit::B
                },
                ..Default::default()
            }
            .semantic()
        );
    }
}
