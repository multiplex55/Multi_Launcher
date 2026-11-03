//! Offline named-zone resolution backed by the bundled IANA database.
//!
//! Local wall times remain unresolved when they are ambiguous or nonexistent.
//! A fold requires an explicit [`FoldChoice`], and a gap is returned with its
//! rejected source and transition boundaries rather than being normalized.

use chrono::{DateTime, LocalResult, NaiveDateTime, Offset, TimeZone, Utc};
use chrono_tz::{GapInfo, OffsetComponents, OffsetName, TZ_VARIANTS, Tz};
use std::str::FromStr;

/// Version of the IANA time-zone database bundled in this build.
pub const ZONE_DATABASE_VERSION: &str = chrono_tz::IANA_TZDB_VERSION;

/// Maximum accepted byte length for a zone name or search query.
pub const MAX_ZONE_TEXT_BYTES: usize = 128;
/// Maximum number of local target zones returned by one conversion request.
pub const MAX_ZONE_TARGETS: usize = 16;
/// Maximum number of time-zone search results.
pub const MAX_ZONE_SEARCH_RESULTS: usize = 50;

/// A parsed, canonical named time zone from the bundled IANA database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NamedTimeZone {
    tz: Tz,
    name: &'static str,
}

impl NamedTimeZone {
    /// Parses an exact IANA identifier or a database alias.
    pub fn parse(input: &str) -> Result<Self, TimeZoneError> {
        if input.len() > MAX_ZONE_TEXT_BYTES {
            return Err(TimeZoneError::TextTooLong {
                max_bytes: MAX_ZONE_TEXT_BYTES,
            });
        }
        let name = input.trim();
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err(TimeZoneError::InvalidName {
                input: name.to_owned(),
            });
        }
        let tz = Tz::from_str(name).map_err(|_| TimeZoneError::UnknownName {
            input: name.to_owned(),
        })?;
        Ok(Self {
            tz,
            name: tz.name(),
        })
    }

    /// Canonical database identifier for this zone.
    pub const fn name(self) -> &'static str {
        self.name
    }
}

/// Bounded, deterministic search through the bundled zone identifiers.
pub fn search_time_zones(query: &str, limit: usize) -> Result<Vec<NamedTimeZone>, TimeZoneError> {
    if query.len() > MAX_ZONE_TEXT_BYTES {
        return Err(TimeZoneError::TextTooLong {
            max_bytes: MAX_ZONE_TEXT_BYTES,
        });
    }
    if limit == 0 || limit > MAX_ZONE_SEARCH_RESULTS {
        return Err(TimeZoneError::InvalidSearchLimit {
            requested: limit,
            maximum: MAX_ZONE_SEARCH_RESULTS,
        });
    }

    let query = query.trim().to_lowercase();
    let mut matches = Vec::with_capacity(TZ_VARIANTS.len());
    for tz in TZ_VARIANTS {
        let name = tz.name();
        if name.to_lowercase().contains(&query) {
            matches.push(NamedTimeZone { tz, name });
        }
    }
    matches.sort_unstable_by_key(|zone| zone.name);
    matches.dedup_by_key(|zone| zone.name);
    matches.truncate(limit);
    Ok(matches)
}

/// Which side of a daylight-saving fold to use when explicitly resolving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldChoice {
    Earlier,
    Later,
}

/// A local wall time paired with the exact UTC instant and offset that resolve it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZonedDateTime {
    pub zone: NamedTimeZone,
    pub instant_utc: DateTime<Utc>,
    pub local: NaiveDateTime,
    pub offset_seconds: i32,
    pub dst_offset_seconds: i32,
    pub abbreviation: Option<String>,
}

impl ZonedDateTime {
    /// Human-readable label including canonical zone, numeric offset and DST state.
    pub fn display_label(&self) -> String {
        let abbreviation = self.abbreviation.as_deref().unwrap_or(self.zone.name());
        let daylight = if self.dst_offset_seconds == 0 {
            "standard time"
        } else {
            "daylight time"
        };
        format!(
            "{} {} {} ({}) — {}",
            self.local.format("%Y-%m-%d %H:%M:%S%.f"),
            abbreviation,
            format_utc_offset(self.offset_seconds),
            self.zone.name(),
            daylight
        )
    }
}

/// The boundary at which a nonexistent local-time interval begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapStartBoundary {
    /// First local wall time in the rejected gap.
    pub local: NaiveDateTime,
    /// Offset in use immediately before the transition.
    pub prior_offset_seconds: i32,
    pub prior_abbreviation: Option<String>,
}

/// Explicit result of interpreting a wall time in a named zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalResolution {
    Single(ZonedDateTime),
    Ambiguous {
        requested_local: NaiveDateTime,
        zone: NamedTimeZone,
        earlier: ZonedDateTime,
        later: ZonedDateTime,
    },
    Nonexistent {
        rejected_local: NaiveDateTime,
        zone: NamedTimeZone,
        gap_start: Option<GapStartBoundary>,
        gap_end: Option<ZonedDateTime>,
        explanation: String,
    },
}

impl LocalResolution {
    /// Resolves a single time directly or a fold only when a side is supplied.
    /// A nonexistent wall time always remains an error requiring another input.
    pub fn select(&self, fold_choice: Option<FoldChoice>) -> Result<ZonedDateTime, TimeZoneError> {
        match self {
            Self::Single(value) => Ok(value.clone()),
            Self::Ambiguous {
                requested_local,
                zone,
                earlier,
                later,
            } => match fold_choice {
                Some(FoldChoice::Earlier) => Ok(earlier.clone()),
                Some(FoldChoice::Later) => Ok(later.clone()),
                None => Err(TimeZoneError::FoldChoiceRequired {
                    zone: zone.name().to_owned(),
                    local: *requested_local,
                }),
            },
            Self::Nonexistent {
                rejected_local,
                zone,
                ..
            } => Err(TimeZoneError::NonexistentLocalTime {
                zone: zone.name().to_owned(),
                local: *rejected_local,
            }),
        }
    }
}

/// Resolves a wall time without guessing a fold or repairing a gap.
///
/// Returns a range error when a valid UTC instant or its local projection is
/// outside chrono's representable date-time range. That condition is distinct
/// from a daylight-saving gap.
pub fn resolve_local_datetime(
    local: NaiveDateTime,
    zone: NamedTimeZone,
) -> Result<LocalResolution, TimeZoneError> {
    match zone.tz.from_local_datetime(&local) {
        LocalResult::Single(value) => Ok(LocalResolution::Single(from_zone_datetime(value, zone)?)),
        LocalResult::Ambiguous(first, second) => {
            let first = from_zone_datetime(first, zone)?;
            let second = from_zone_datetime(second, zone)?;
            let (earlier, later) = if first.instant_utc <= second.instant_utc {
                (first, second)
            } else {
                (second, first)
            };
            Ok(LocalResolution::Ambiguous {
                requested_local: local,
                zone,
                earlier,
                later,
            })
        }
        LocalResult::None => {
            let info = GapInfo::new(&local, &zone.tz).ok_or_else(|| {
                TimeZoneError::LocalResolutionOutOfRange {
                    zone: zone.name().to_owned(),
                    local,
                }
            })?;
            let gap_start = info
                .begin
                .as_ref()
                .map(|(boundary, offset)| GapStartBoundary {
                    local: *boundary,
                    prior_offset_seconds: offset.fix().local_minus_utc(),
                    prior_abbreviation: offset.abbreviation().map(str::to_owned),
                });
            let gap_end = info
                .end
                .map(|value| from_zone_datetime(value, zone))
                .transpose()?;
            let explanation = match (&gap_start, &gap_end) {
                (Some(start), Some(end)) => format!(
                    "{} does not exist in {}; the local-time gap starts at {} and resumes at {}.",
                    local,
                    zone.name(),
                    start.local,
                    end.local
                ),
                _ => format!(
                    "{} does not exist in {}; the local-time transition boundary is unavailable.",
                    local,
                    zone.name()
                ),
            };
            Ok(LocalResolution::Nonexistent {
                rejected_local: local,
                zone,
                gap_start,
                gap_end,
                explanation,
            })
        }
    }
}

/// Converts an explicitly resolved source instant into bounded target zones.
/// Every result is derived from the same UTC instant, including across DST changes.
/// A target whose local projection exceeds chrono's date-time range is returned
/// as a recoverable [`TimeZoneError::LocalProjectionOutOfRange`].
pub fn convert_to_time_zones(
    source: &ZonedDateTime,
    targets: &[NamedTimeZone],
) -> Result<Vec<ZonedDateTime>, TimeZoneError> {
    if targets.len() > MAX_ZONE_TARGETS {
        return Err(TimeZoneError::TooManyTargets {
            requested: targets.len(),
            maximum: MAX_ZONE_TARGETS,
        });
    }
    let utc = source.instant_utc.naive_utc();
    targets
        .iter()
        .map(|zone| from_zone_datetime(zone.tz.from_utc_datetime(&utc), *zone))
        .collect()
}

fn from_zone_datetime(
    value: DateTime<Tz>,
    zone: NamedTimeZone,
) -> Result<ZonedDateTime, TimeZoneError> {
    let offset = value.offset();
    let instant_utc = value.with_timezone(&Utc);
    let fixed_offset = offset.fix();
    let offset_seconds = fixed_offset.local_minus_utc();
    let local = instant_utc
        .naive_utc()
        .checked_add_offset(fixed_offset)
        .ok_or_else(|| TimeZoneError::LocalProjectionOutOfRange {
            zone: zone.name().to_owned(),
            instant_utc: instant_utc.naive_utc(),
            offset_seconds,
        })?;
    Ok(ZonedDateTime {
        zone,
        instant_utc,
        local,
        offset_seconds,
        dst_offset_seconds: offset.dst_offset().num_seconds() as i32,
        abbreviation: offset.abbreviation().map(str::to_owned),
    })
}

fn format_utc_offset(seconds: i32) -> String {
    let seconds = i64::from(seconds);
    let sign = if seconds < 0 { '-' } else { '+' };
    let magnitude = seconds.abs();
    let hours = magnitude / 3_600;
    let minutes = magnitude % 3_600 / 60;
    let seconds = magnitude % 60;
    if seconds == 0 {
        format!("UTC{sign}{hours:02}:{minutes:02}")
    } else {
        format!("UTC{sign}{hours:02}:{minutes:02}:{seconds:02}")
    }
}

/// Recoverable errors from named-zone parsing and local-time resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeZoneError {
    TextTooLong {
        max_bytes: usize,
    },
    InvalidName {
        input: String,
    },
    UnknownName {
        input: String,
    },
    InvalidSearchLimit {
        requested: usize,
        maximum: usize,
    },
    TooManyTargets {
        requested: usize,
        maximum: usize,
    },
    FoldChoiceRequired {
        zone: String,
        local: NaiveDateTime,
    },
    NonexistentLocalTime {
        zone: String,
        local: NaiveDateTime,
    },
    LocalResolutionOutOfRange {
        zone: String,
        local: NaiveDateTime,
    },
    LocalProjectionOutOfRange {
        zone: String,
        instant_utc: NaiveDateTime,
        offset_seconds: i32,
    },
}

impl std::fmt::Display for TimeZoneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TextTooLong { max_bytes } => {
                write!(formatter, "time-zone text exceeds {max_bytes} bytes")
            }
            Self::InvalidName { input } => write!(formatter, "invalid time-zone name: {input}"),
            Self::UnknownName { input } => write!(formatter, "unknown time zone: {input}"),
            Self::InvalidSearchLimit { requested, maximum } => write!(
                formatter,
                "time-zone search limit {requested} is outside 1..={maximum}"
            ),
            Self::TooManyTargets { requested, maximum } => write!(
                formatter,
                "requested {requested} target zones; the maximum is {maximum}"
            ),
            Self::FoldChoiceRequired { zone, local } => write!(
                formatter,
                "{local} is ambiguous in {zone}; choose the earlier or later occurrence"
            ),
            Self::NonexistentLocalTime { zone, local } => {
                write!(formatter, "{local} does not exist in {zone}")
            }
            Self::LocalResolutionOutOfRange { zone, local } => write!(
                formatter,
                "{local} cannot be resolved in {zone} within the supported date range"
            ),
            Self::LocalProjectionOutOfRange {
                zone,
                instant_utc,
                offset_seconds,
            } => write!(
                formatter,
                "UTC instant {instant_utc} with offset {offset_seconds} seconds is outside the local date range for {zone}"
            ),
        }
    }
}

impl std::error::Error for TimeZoneError {}

#[cfg(test)]
mod tests {
    use super::{
        FoldChoice, LocalResolution, MAX_ZONE_TARGETS, MAX_ZONE_TEXT_BYTES, NamedTimeZone,
        TimeZoneError, ZONE_DATABASE_VERSION, convert_to_time_zones, resolve_local_datetime,
        search_time_zones,
    };
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

    fn local(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(hour, minute, 0).unwrap())
    }

    #[test]
    fn named_zones_are_bounded_canonical_and_searchable_offline() {
        let london = NamedTimeZone::parse(" Europe/London ").unwrap();
        assert_eq!(london.name(), "Europe/London");
        assert_eq!(ZONE_DATABASE_VERSION, "2025b");
        assert!(
            search_time_zones("new_york", 10)
                .unwrap()
                .iter()
                .any(|zone| zone.name() == "America/New_York")
        );
        assert!(matches!(
            NamedTimeZone::parse(&"x".repeat(MAX_ZONE_TEXT_BYTES + 1)),
            Err(TimeZoneError::TextTooLong { .. })
        ));
        assert!(matches!(
            NamedTimeZone::parse("Mars/Olympus_Mons"),
            Err(TimeZoneError::UnknownName { .. })
        ));
        assert!(matches!(
            search_time_zones("", 0),
            Err(TimeZoneError::InvalidSearchLimit { .. })
        ));
        assert!(matches!(
            search_time_zones(&"z".repeat(MAX_ZONE_TEXT_BYTES + 1), 10),
            Err(TimeZoneError::TextTooLong { .. })
        ));
    }

    #[test]
    fn named_zones_report_winter_summer_and_non_hour_offsets() {
        let new_york = NamedTimeZone::parse("America/New_York").unwrap();
        let winter = resolve_local_datetime(local(2026, 1, 15, 12, 0), new_york)
            .unwrap()
            .select(None)
            .unwrap();
        let summer = resolve_local_datetime(local(2026, 7, 15, 12, 0), new_york)
            .unwrap()
            .select(None)
            .unwrap();
        assert_eq!(winter.offset_seconds, -5 * 3_600);
        assert_eq!(winter.dst_offset_seconds, 0);
        assert_eq!(summer.offset_seconds, -4 * 3_600);
        assert_eq!(summer.dst_offset_seconds, 3_600);
        assert!(summer.display_label().contains("daylight time"));

        let kathmandu = NamedTimeZone::parse("Asia/Kathmandu").unwrap();
        let local_time = resolve_local_datetime(local(2026, 1, 15, 12, 0), kathmandu)
            .unwrap()
            .select(None)
            .unwrap();
        assert_eq!(local_time.offset_seconds, 5 * 3_600 + 45 * 60);
    }

    #[test]
    fn target_zones_convert_from_the_same_resolved_utc_instant() {
        let new_york = NamedTimeZone::parse("America/New_York").unwrap();
        let london = NamedTimeZone::parse("Europe/London").unwrap();
        let utc = NamedTimeZone::parse("UTC").unwrap();
        let source = resolve_local_datetime(local(2026, 3, 15, 8, 0), new_york)
            .unwrap()
            .select(None)
            .unwrap();
        // The U.S. has entered daylight time by this date; the U.K. has not.
        assert_eq!(source.offset_seconds, -4 * 3_600);
        let targets = convert_to_time_zones(&source, &[london, utc]).unwrap();
        assert_eq!(targets[0].instant_utc, source.instant_utc);
        assert_eq!(targets[0].local, local(2026, 3, 15, 12, 0));
        assert_eq!(targets[0].offset_seconds, 0);
        assert_eq!(targets[0].dst_offset_seconds, 0);
        assert_eq!(targets[1].local, local(2026, 3, 15, 12, 0));
        assert_eq!(targets[1].instant_utc, source.instant_utc);

        let targets = vec![utc; MAX_ZONE_TARGETS + 1];
        assert!(matches!(
            convert_to_time_zones(&source, &targets),
            Err(TimeZoneError::TooManyTargets { .. })
        ));
    }

    #[test]
    fn target_conversion_preserves_leap_seconds() {
        let utc = NamedTimeZone::parse("UTC").unwrap();
        let target = NamedTimeZone::parse("Etc/GMT-1").unwrap();
        let leap_utc = NaiveDate::from_ymd_opt(2016, 12, 31)
            .unwrap()
            .and_hms_nano_opt(23, 59, 59, 1_300_000_000)
            .unwrap();
        let source = super::ZonedDateTime {
            zone: utc,
            instant_utc: Utc.from_utc_datetime(&leap_utc),
            local: leap_utc,
            offset_seconds: 0,
            dst_offset_seconds: 0,
            abbreviation: Some("UTC".to_owned()),
        };

        let converted = convert_to_time_zones(&source, &[target]).unwrap();
        let expected_local = NaiveDate::from_ymd_opt(2017, 1, 1)
            .unwrap()
            .and_hms_nano_opt(0, 59, 59, 1_300_000_000)
            .unwrap();
        assert_eq!(converted[0].local, expected_local);
        assert_eq!(converted[0].instant_utc, source.instant_utc);
        assert_eq!(converted[0].offset_seconds, 3_600);
    }

    #[test]
    fn folds_are_chronological_and_require_an_explicit_choice() {
        let zone = NamedTimeZone::parse("America/New_York").unwrap();
        let rejected = local(2026, 11, 1, 1, 30);
        let resolution = resolve_local_datetime(rejected, zone).unwrap();
        let LocalResolution::Ambiguous {
            requested_local,
            earlier,
            later,
            ..
        } = &resolution
        else {
            panic!("New York fall-back wall time is a fold");
        };
        assert_eq!(*requested_local, rejected);
        assert_eq!(
            earlier.instant_utc,
            Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0).unwrap()
        );
        assert_eq!(
            later.instant_utc,
            Utc.with_ymd_and_hms(2026, 11, 1, 6, 30, 0).unwrap()
        );
        assert_eq!(earlier.offset_seconds, -4 * 3_600);
        assert_eq!(later.offset_seconds, -5 * 3_600);
        assert!(matches!(
            resolution.select(None),
            Err(TimeZoneError::FoldChoiceRequired { .. })
        ));
        assert_eq!(
            &resolution.select(Some(FoldChoice::Earlier)).unwrap(),
            earlier
        );
        assert_eq!(&resolution.select(Some(FoldChoice::Later)).unwrap(), later);
        assert_eq!(*requested_local, rejected);
    }

    #[test]
    fn nonexistent_local_times_keep_the_rejected_input_and_gap_boundaries() {
        let zone = NamedTimeZone::parse("America/New_York").unwrap();
        let rejected = local(2026, 3, 8, 2, 30);
        let resolution = resolve_local_datetime(rejected, zone).unwrap();
        let LocalResolution::Nonexistent {
            rejected_local,
            gap_start,
            gap_end,
            explanation,
            ..
        } = resolution
        else {
            panic!("New York spring-forward wall time is a gap");
        };
        assert_eq!(rejected_local, rejected);
        let start = gap_start.unwrap();
        assert_eq!(start.local, local(2026, 3, 8, 2, 0));
        assert_eq!(start.prior_offset_seconds, -5 * 3_600);
        let end = gap_end.unwrap();
        assert_eq!(end.local, local(2026, 3, 8, 3, 0));
        assert_eq!(end.offset_seconds, -4 * 3_600);
        assert!(explanation.contains("does not exist"));
        assert!(matches!(
            resolve_local_datetime(rejected, zone).unwrap().select(None),
            Err(TimeZoneError::NonexistentLocalTime { local, .. }) if local == rejected
        ));
    }

    #[test]
    fn local_resolution_and_target_projection_report_range_edges_separately_from_dst_gaps() {
        let positive = NamedTimeZone::parse("Etc/GMT-14").unwrap();
        let negative = NamedTimeZone::parse("Etc/GMT+12").unwrap();
        let minimum = NaiveDateTime::MIN;
        let maximum = NaiveDateTime::MAX;

        assert!(matches!(
            resolve_local_datetime(minimum, positive),
            Err(TimeZoneError::LocalResolutionOutOfRange { local, .. }) if local == minimum
        ));
        assert!(matches!(
            resolve_local_datetime(maximum, negative),
            Err(TimeZoneError::LocalResolutionOutOfRange { local, .. }) if local == maximum
        ));

        let utc = NamedTimeZone::parse("UTC").unwrap();
        let source_max = super::ZonedDateTime {
            zone: utc,
            instant_utc: maximum.and_utc(),
            local: maximum,
            offset_seconds: 0,
            dst_offset_seconds: 0,
            abbreviation: Some("UTC".to_owned()),
        };
        assert!(matches!(
            convert_to_time_zones(&source_max, &[positive]),
            Err(TimeZoneError::LocalProjectionOutOfRange {
                instant_utc,
                offset_seconds,
                ..
            }) if instant_utc == maximum && offset_seconds == 14 * 3_600
        ));

        let source_min = super::ZonedDateTime {
            instant_utc: minimum.and_utc(),
            local: minimum,
            ..source_max
        };
        assert!(matches!(
            convert_to_time_zones(&source_min, &[negative]),
            Err(TimeZoneError::LocalProjectionOutOfRange {
                instant_utc,
                offset_seconds,
                ..
            }) if instant_utc == minimum && offset_seconds == -12 * 3_600
        ));
    }
}
