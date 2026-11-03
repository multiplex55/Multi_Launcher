//! Structured, side-effect-free date and time operations.
//!
//! Relative inputs always receive their reference date-time from the caller.
//! Calendar offsets reuse the existing clipped-month implementation. Date/time
//! differences preserve their signed duration, while [`CalendarSpan`] reports
//! complete clipped months followed by a nonnegative remainder; it is a display
//! convention and is not promised to invert calendar subtraction.

pub use super::zones::*;
use super::{
    DateArithmeticError, DateOffsetRequest, DateValue, apply_offset_request, difference_days,
    shift_calendar_months,
};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};

/// Maximum occurrences produced by one recurrence preview.
pub const MAX_SCHEDULE_OCCURRENCES: usize = 32;
/// Maximum supported span between the first and final preview dates.
pub const MAX_SCHEDULE_HORIZON_DAYS: i64 = 366;

/// Errors from structured date/time operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateStudioError {
    Offset(DateArithmeticError),
    TimeZone(TimeZoneError),
    IncompatibleDateKinds,
    ExplicitTimeForDateTime,
    InvalidScheduleCount { requested: usize, maximum: usize },
    ScheduleHorizonExceeded { maximum_days: i64 },
    OutOfRange,
}

impl std::fmt::Display for DateStudioError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offset(error) => write!(formatter, "{error}"),
            Self::TimeZone(error) => write!(formatter, "{error}"),
            Self::IncompatibleDateKinds => write!(
                formatter,
                "between endpoints must both be dates or both be date-times"
            ),
            Self::ExplicitTimeForDateTime => write!(
                formatter,
                "an explicit comparison time can only be supplied for a date endpoint"
            ),
            Self::InvalidScheduleCount { requested, maximum } => write!(
                formatter,
                "schedule preview count {requested} is outside 1..={maximum}"
            ),
            Self::ScheduleHorizonExceeded { maximum_days } => write!(
                formatter,
                "schedule preview exceeds the {maximum_days}-day horizon"
            ),
            Self::OutOfRange => write!(
                formatter,
                "date/time operation is outside the supported range"
            ),
        }
    }
}

impl std::error::Error for DateStudioError {}

impl From<TimeZoneError> for DateStudioError {
    fn from(error: TimeZoneError) -> Self {
        Self::TimeZone(error)
    }
}

/// One endpoint for a structured between calculation. An optional time turns
/// a date into a date-time; it never overrides a date-time's existing time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateBetweenEndpoint {
    pub value: DateValue,
    pub explicit_time: Option<NaiveTime>,
}

impl DateBetweenEndpoint {
    pub const fn new(value: DateValue) -> Self {
        Self {
            value,
            explicit_time: None,
        }
    }

    pub const fn with_time(value: DateValue, explicit_time: NaiveTime) -> Self {
        Self {
            value,
            explicit_time: Some(explicit_time),
        }
    }
}

/// Typed inputs for a signed between operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateBetweenRequest {
    pub first: DateBetweenEndpoint,
    pub second: DateBetweenEndpoint,
}

impl DateBetweenRequest {
    pub const fn new(first: DateValue, second: DateValue) -> Self {
        Self {
            first: DateBetweenEndpoint::new(first),
            second: DateBetweenEndpoint::new(second),
        }
    }
}

/// Calendar month count plus a nonnegative duration remainder.
///
/// Apply `sign` to both nonnegative components to express the ordered result.
/// The complete month count clips the earlier endpoint's day when necessary,
/// then `remaining` is exact elapsed duration. Calendar months are not a fixed
/// number of days, and this representation is not the inverse of subtraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarSpan {
    pub sign: i8,
    pub complete_months: u64,
    pub remaining: Duration,
}

/// Complete typed result for a signed between request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DateBetweenResult {
    pub first: DateValue,
    pub second: DateValue,
    pub exact_duration: Duration,
    pub days: f64,
    pub weeks: f64,
    pub calendar_span: CalendarSpan,
}

/// Calculates signed elapsed time, day/week magnitudes and a clipped calendar span.
pub fn evaluate_between(request: DateBetweenRequest) -> Result<DateBetweenResult, DateStudioError> {
    let first = normalize_endpoint(request.first)?;
    let second = normalize_endpoint(request.second)?;
    let (first, second) = match (first, second) {
        (DateValue::Date(first), DateValue::Date(second)) => {
            (DateValue::Date(first), DateValue::Date(second))
        }
        (DateValue::DateTime(first), DateValue::DateTime(second)) => {
            (DateValue::DateTime(first), DateValue::DateTime(second))
        }
        _ => return Err(DateStudioError::IncompatibleDateKinds),
    };
    let days =
        difference_days(first, second, "structured between").map_err(DateStudioError::Offset)?;
    if !days.is_finite() {
        return Err(DateStudioError::OutOfRange);
    }
    let first_datetime = at_midnight_or_value(first);
    let second_datetime = at_midnight_or_value(second);
    let exact_duration = second_datetime.signed_duration_since(first_datetime);
    let calendar_span = calculate_calendar_span(first_datetime, second_datetime)?;
    Ok(DateBetweenResult {
        first,
        second,
        exact_duration,
        days,
        weeks: days / 7.0,
        calendar_span,
    })
}

fn normalize_endpoint(endpoint: DateBetweenEndpoint) -> Result<DateValue, DateStudioError> {
    match (endpoint.value, endpoint.explicit_time) {
        (DateValue::Date(date), Some(time)) => Ok(DateValue::DateTime(date.and_time(time))),
        (DateValue::Date(date), None) => Ok(DateValue::Date(date)),
        (DateValue::DateTime(_), Some(_)) => Err(DateStudioError::ExplicitTimeForDateTime),
        (DateValue::DateTime(date_time), None) => Ok(DateValue::DateTime(date_time)),
    }
}

fn at_midnight_or_value(value: DateValue) -> NaiveDateTime {
    match value {
        DateValue::Date(date) => date.and_time(NaiveTime::MIN),
        DateValue::DateTime(date_time) => date_time,
    }
}

fn calculate_calendar_span(
    first: NaiveDateTime,
    second: NaiveDateTime,
) -> Result<CalendarSpan, DateStudioError> {
    use std::cmp::Ordering;

    let (earlier, later, sign) = match first.cmp(&second) {
        Ordering::Less => (first, second, 1),
        Ordering::Greater => (second, first, -1),
        Ordering::Equal => {
            return Ok(CalendarSpan {
                sign: 0,
                complete_months: 0,
                remaining: Duration::zero(),
            });
        }
    };

    let earlier_index = i64::from(earlier.year())
        .checked_mul(12)
        .and_then(|index| index.checked_add(i64::from(earlier.month0())))
        .ok_or(DateStudioError::OutOfRange)?;
    let later_index = i64::from(later.year())
        .checked_mul(12)
        .and_then(|index| index.checked_add(i64::from(later.month0())))
        .ok_or(DateStudioError::OutOfRange)?;
    let mut months = later_index
        .checked_sub(earlier_index)
        .ok_or(DateStudioError::OutOfRange)?;
    let mut anniversary = shift_month_datetime(earlier, months)?;
    while anniversary > later {
        months = months.checked_sub(1).ok_or(DateStudioError::OutOfRange)?;
        anniversary = shift_month_datetime(earlier, months)?;
    }
    let remaining = later.signed_duration_since(anniversary);
    Ok(CalendarSpan {
        sign,
        complete_months: u64::try_from(months).map_err(|_| DateStudioError::OutOfRange)?,
        remaining,
    })
}

fn shift_month_datetime(
    value: NaiveDateTime,
    months: i64,
) -> Result<NaiveDateTime, DateStudioError> {
    let date = shift_calendar_months(value.date(), months).ok_or(DateStudioError::OutOfRange)?;
    Ok(date.and_time(value.time()))
}

/// Daily or weekly calendar-date stepping for a bounded preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleCadence {
    Daily,
    Weekly,
}

/// Inputs for a local-time schedule preview. This does not create or persist a schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulePreviewRequest {
    pub first_local: NaiveDateTime,
    pub zone: NamedTimeZone,
    pub cadence: ScheduleCadence,
    pub count: usize,
}

/// One requested calendar occurrence and its unresolved zone interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleOccurrence {
    pub index: usize,
    pub requested_local: NaiveDateTime,
    pub resolution: LocalResolution,
}

/// Previews calendar-based occurrences without choosing folds, normalizing gaps,
/// or skipping any requested date.
pub fn preview_schedule(
    request: SchedulePreviewRequest,
) -> Result<Vec<ScheduleOccurrence>, DateStudioError> {
    if request.count == 0 || request.count > MAX_SCHEDULE_OCCURRENCES {
        return Err(DateStudioError::InvalidScheduleCount {
            requested: request.count,
            maximum: MAX_SCHEDULE_OCCURRENCES,
        });
    }
    let days_per_occurrence = match request.cadence {
        ScheduleCadence::Daily => 1_i64,
        ScheduleCadence::Weekly => 7_i64,
    };
    let final_index = i64::try_from(request.count - 1).map_err(|_| DateStudioError::OutOfRange)?;
    let final_day = final_index
        .checked_mul(days_per_occurrence)
        .ok_or(DateStudioError::OutOfRange)?;
    if final_day > MAX_SCHEDULE_HORIZON_DAYS {
        return Err(DateStudioError::ScheduleHorizonExceeded {
            maximum_days: MAX_SCHEDULE_HORIZON_DAYS,
        });
    }

    let mut occurrences = Vec::with_capacity(request.count);
    for index in 0..request.count {
        let day_offset = i64::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(days_per_occurrence))
            .ok_or(DateStudioError::OutOfRange)?;
        let duration = Duration::try_days(day_offset).ok_or(DateStudioError::OutOfRange)?;
        let date = request
            .first_local
            .date()
            .checked_add_signed(duration)
            .ok_or(DateStudioError::OutOfRange)?;
        let requested_local = date.and_time(request.first_local.time());
        occurrences.push(ScheduleOccurrence {
            index,
            requested_local,
            resolution: resolve_local_datetime(requested_local, request.zone)?,
        });
    }
    Ok(occurrences)
}

#[cfg(test)]
mod tests {
    use super::{
        DateBetweenEndpoint, DateBetweenRequest, DateStudioError, MAX_SCHEDULE_OCCURRENCES,
        NamedTimeZone, ScheduleCadence, SchedulePreviewRequest, apply_offset_request,
        evaluate_between, preview_schedule,
    };
    use crate::date_arithmetic::{
        DateArithmeticError, DateOffsetRequest, DateOffsetUnit, DateResult, DateValue,
        evaluate_expression,
    };
    use chrono::{Duration, NaiveDate, NaiveTime};

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn time(hour: u32, minute: u32, second: u32, nanosecond: u32) -> NaiveTime {
        NaiveTime::from_hms_nano_opt(hour, minute, second, nanosecond).unwrap()
    }

    #[test]
    fn structured_offsets_share_legacy_clipping_and_preserve_time_precision() {
        let january_end = DateValue::Date(date(2024, 1, 31));
        let structured = apply_offset_request(
            january_end,
            DateOffsetRequest {
                amount: 1,
                unit: DateOffsetUnit::Month,
            },
        )
        .unwrap();
        let legacy = evaluate_expression(
            "1 month after 2024-01-31",
            date(2026, 1, 1).and_time(time(0, 0, 0, 0)),
        )
        .unwrap();
        assert_eq!(structured, DateValue::Date(date(2024, 2, 29)));
        assert_eq!(legacy.result, DateResult::Date(date(2024, 2, 29)));

        let start = DateValue::DateTime(date(2024, 1, 31).and_time(time(9, 45, 12, 345_678_901)));
        assert_eq!(
            apply_offset_request(
                start,
                DateOffsetRequest {
                    amount: 1,
                    unit: DateOffsetUnit::Month,
                }
            )
            .unwrap(),
            DateValue::DateTime(date(2024, 2, 29).and_time(time(9, 45, 12, 345_678_901)))
        );
        assert_eq!(
            apply_offset_request(
                start,
                DateOffsetRequest {
                    amount: 1,
                    unit: DateOffsetUnit::Nanosecond,
                }
            )
            .unwrap(),
            DateValue::DateTime(date(2024, 1, 31).and_time(time(9, 45, 12, 345_678_902)))
        );

        let long_offset_datetime = date(2026, 1, 1).and_time(time(0, 0, 0, 0));
        let long_offset_anchor = DateValue::DateTime(long_offset_datetime);
        let long_milliseconds = 10_000_000_000_000_i64;
        let expected_milliseconds = DateValue::DateTime(
            long_offset_datetime
                .checked_add_signed(Duration::try_milliseconds(long_milliseconds).unwrap())
                .unwrap(),
        );
        assert_eq!(
            apply_offset_request(
                long_offset_anchor,
                DateOffsetRequest {
                    amount: long_milliseconds,
                    unit: DateOffsetUnit::Millisecond,
                }
            )
            .unwrap(),
            expected_milliseconds
        );

        let long_microseconds = 10_000_000_000_000_000_i64;
        let expected_microseconds = DateValue::DateTime(
            long_offset_datetime
                .checked_add_signed(Duration::microseconds(long_microseconds))
                .unwrap(),
        );
        assert_eq!(
            apply_offset_request(
                long_offset_anchor,
                DateOffsetRequest {
                    amount: long_microseconds,
                    unit: DateOffsetUnit::Microsecond,
                }
            )
            .unwrap(),
            expected_microseconds
        );
    }

    #[test]
    fn structured_offsets_cover_negative_clipping_and_checked_range_errors() {
        let start = DateValue::Date(date(2024, 3, 31));
        assert_eq!(
            apply_offset_request(
                start,
                DateOffsetRequest {
                    amount: -1,
                    unit: DateOffsetUnit::Month,
                }
            )
            .unwrap(),
            DateValue::Date(date(2024, 2, 29))
        );
        assert!(matches!(
            apply_offset_request(
                start,
                DateOffsetRequest {
                    amount: 1,
                    unit: DateOffsetUnit::Hour,
                }
            ),
            Err(DateArithmeticError::TimeRequired { .. })
        ));
        assert!(matches!(
            apply_offset_request(
                DateValue::Date(NaiveDate::MAX),
                DateOffsetRequest {
                    amount: 1,
                    unit: DateOffsetUnit::Day,
                }
            ),
            Err(DateArithmeticError::OutOfRange { .. })
        ));
        assert!(matches!(
            apply_offset_request(
                DateValue::Date(date(2024, 1, 1)),
                DateOffsetRequest {
                    amount: i64::MAX,
                    unit: DateOffsetUnit::Year,
                }
            ),
            Err(DateArithmeticError::OutOfRange { .. })
        ));
        let final_instant =
            DateValue::DateTime(NaiveDate::MAX.and_time(time(23, 59, 59, 999_999_999)));
        assert!(matches!(
            apply_offset_request(
                final_instant,
                DateOffsetRequest {
                    amount: 1,
                    unit: DateOffsetUnit::Nanosecond,
                }
            ),
            Err(DateArithmeticError::OutOfRange { .. })
        ));
        assert!(matches!(
            apply_offset_request(
                DateValue::DateTime(NaiveDate::MAX.and_time(time(23, 59, 59, 999_999_999))),
                DateOffsetRequest {
                    amount: 1,
                    unit: DateOffsetUnit::Millisecond,
                }
            ),
            Err(DateArithmeticError::OutOfRange { .. })
        ));
        assert!(matches!(
            apply_offset_request(
                DateValue::DateTime(NaiveDate::MAX.and_time(time(23, 59, 59, 999_999_999))),
                DateOffsetRequest {
                    amount: 1,
                    unit: DateOffsetUnit::Microsecond,
                }
            ),
            Err(DateArithmeticError::OutOfRange { .. })
        ));
        assert!(matches!(
            apply_offset_request(
                DateValue::DateTime(date(2024, 1, 1).and_time(time(0, 0, 0, 0))),
                DateOffsetRequest {
                    amount: i64::MAX,
                    unit: DateOffsetUnit::Second,
                }
            ),
            Err(DateArithmeticError::OutOfRange { .. })
        ));
    }

    #[test]
    fn between_reports_exact_signed_duration_days_weeks_and_calendar_span() {
        let start = DateValue::DateTime(date(2026, 1, 31).and_time(time(12, 0, 0, 0)));
        let end = DateValue::DateTime(date(2026, 3, 30).and_time(time(12, 0, 0, 123_000_000)));
        let result = evaluate_between(DateBetweenRequest::new(start, end)).unwrap();
        assert_eq!(
            result.exact_duration,
            Duration::days(58) + Duration::milliseconds(123)
        );
        assert!((result.days - (58.0 + 0.123 / 86_400.0)).abs() < 1e-12);
        assert!((result.weeks - result.days / 7.0).abs() < 1e-12);
        assert_eq!(result.calendar_span.sign, 1);
        assert_eq!(result.calendar_span.complete_months, 1);
        assert_eq!(
            result.calendar_span.remaining,
            Duration::days(30) + Duration::milliseconds(123)
        );

        let reversed = evaluate_between(DateBetweenRequest::new(end, start)).unwrap();
        assert_eq!(reversed.exact_duration, -result.exact_duration);
        assert!((reversed.days + result.days).abs() < 1e-12);
        assert_eq!(reversed.calendar_span.sign, -1);
        assert_eq!(reversed.calendar_span.complete_months, 1);
        assert_eq!(
            reversed.calendar_span.remaining,
            result.calendar_span.remaining
        );
    }

    #[test]
    fn between_preserves_overnight_fractional_duration() {
        let first = DateValue::DateTime(date(2026, 4, 2).and_time(time(23, 30, 0, 500_000_000)));
        let second = DateValue::DateTime(date(2026, 4, 3).and_time(time(1, 15, 0, 750_000_000)));
        let result = evaluate_between(DateBetweenRequest::new(first, second)).unwrap();

        assert_eq!(
            result.exact_duration,
            Duration::hours(1) + Duration::minutes(45) + Duration::milliseconds(250)
        );
        assert!(result.days > 0.0 && result.days < 1.0);
        assert_eq!(result.calendar_span.sign, 1);
        assert_eq!(result.calendar_span.complete_months, 0);
        assert_eq!(result.calendar_span.remaining, result.exact_duration);
    }

    #[test]
    fn between_rejects_mixed_kinds_unless_a_date_endpoint_gets_an_explicit_time() {
        let day = DateValue::Date(date(2026, 4, 2));
        let moment = DateValue::DateTime(date(2026, 4, 2).and_time(time(18, 30, 0, 0)));
        assert!(matches!(
            evaluate_between(DateBetweenRequest::new(day, moment)),
            Err(DateStudioError::IncompatibleDateKinds)
        ));
        let allowed = DateBetweenRequest {
            first: DateBetweenEndpoint::with_time(day, time(9, 0, 0, 0)),
            second: DateBetweenEndpoint::new(moment),
        };
        let result = evaluate_between(allowed).unwrap();
        assert!((result.days - 9.5 / 24.0).abs() < 1e-12);
        assert!(matches!(result.first, DateValue::DateTime(_)));

        let invalid_override = DateBetweenRequest {
            first: DateBetweenEndpoint::with_time(moment, time(9, 0, 0, 0)),
            second: DateBetweenEndpoint::new(moment),
        };
        assert!(matches!(
            evaluate_between(invalid_override),
            Err(DateStudioError::ExplicitTimeForDateTime)
        ));
    }

    #[test]
    fn date_between_uses_calendar_days_and_zero_has_no_sign() {
        let first = DateValue::Date(date(2024, 2, 28));
        let second = DateValue::Date(date(2024, 3, 1));
        let result = evaluate_between(DateBetweenRequest::new(first, second)).unwrap();
        assert_eq!(result.days, 2.0);
        assert_eq!(result.exact_duration, Duration::days(2));

        let zero = evaluate_between(DateBetweenRequest::new(first, first)).unwrap();
        assert_eq!(zero.calendar_span.sign, 0);
        assert_eq!(zero.calendar_span.remaining, Duration::zero());
    }

    #[test]
    fn daily_and_weekly_previews_step_calendar_dates_and_keep_dst_choices_explicit() {
        let zone = NamedTimeZone::parse("America/New_York").unwrap();
        let daily = preview_schedule(SchedulePreviewRequest {
            first_local: date(2024, 2, 28).and_time(time(9, 15, 0, 0)),
            zone,
            cadence: ScheduleCadence::Daily,
            count: 4,
        })
        .unwrap();
        assert_eq!(
            daily[0].requested_local,
            date(2024, 2, 28).and_time(time(9, 15, 0, 0))
        );
        assert_eq!(
            daily[1].requested_local,
            date(2024, 2, 29).and_time(time(9, 15, 0, 0))
        );
        assert_eq!(
            daily[3].requested_local,
            date(2024, 3, 2).and_time(time(9, 15, 0, 0))
        );

        let spring_gap = preview_schedule(SchedulePreviewRequest {
            first_local: date(2026, 3, 7).and_time(time(2, 30, 0, 0)),
            zone,
            cadence: ScheduleCadence::Daily,
            count: 3,
        })
        .unwrap();
        assert!(matches!(
            &spring_gap[1].resolution,
            super::LocalResolution::Nonexistent { .. }
        ));
        assert_eq!(
            spring_gap[1].requested_local,
            date(2026, 3, 8).and_time(time(2, 30, 0, 0))
        );
        assert!(matches!(
            &spring_gap[2].resolution,
            super::LocalResolution::Single(_)
        ));

        let weekly_fold = preview_schedule(SchedulePreviewRequest {
            first_local: date(2026, 10, 25).and_time(time(1, 30, 0, 0)),
            zone,
            cadence: ScheduleCadence::Weekly,
            count: 2,
        })
        .unwrap();
        assert!(matches!(
            &weekly_fold[1].resolution,
            super::LocalResolution::Ambiguous { .. }
        ));
    }

    #[test]
    fn schedule_preview_rejects_unbounded_counts_and_checked_date_overflow() {
        let zone = NamedTimeZone::parse("UTC").unwrap();
        let mut request = SchedulePreviewRequest {
            first_local: date(2026, 1, 1).and_time(time(12, 0, 0, 0)),
            zone,
            cadence: ScheduleCadence::Daily,
            count: MAX_SCHEDULE_OCCURRENCES + 1,
        };
        assert!(matches!(
            preview_schedule(request),
            Err(DateStudioError::InvalidScheduleCount { .. })
        ));
        request.count = 3;
        request.first_local = NaiveDate::MAX.and_time(time(12, 0, 0, 0));
        assert!(matches!(
            preview_schedule(request),
            Err(DateStudioError::OutOfRange)
        ));
    }

    #[test]
    fn repeated_relative_evaluations_are_deterministic_for_the_same_reference() {
        let reference = date(2026, 10, 5).and_time(time(14, 30, 0, 123_000_000));
        let first = evaluate_expression("3 hours from now", reference).unwrap();
        let second = evaluate_expression("3 hours from now", reference).unwrap();
        assert_eq!(first, second);
    }
}
