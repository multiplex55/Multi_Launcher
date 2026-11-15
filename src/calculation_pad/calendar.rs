//! Deterministic calendar values for the calculation language.
use std::cmp::Ordering;

use chrono::{
    DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone,
    Timelike, Utc, Weekday,
};

use super::document::{CalculationValue, DiagnosticKind};
use super::quantity;
use crate::date_arithmetic::{
    shift_calendar_months,
    zones::{FoldChoice, LocalResolution, NamedTimeZone, resolve_local_datetime},
};
use crate::unit_conversion::{self, Unit};

#[derive(Debug, Clone, PartialEq)]
pub enum CalendarValue {
    Date(NaiveDate),
    Time(NaiveTime),
    Offset(DateTime<FixedOffset>),
    Zoned {
        instant: DateTime<Utc>,
        zone: NamedTimeZone,
    },
    Weekday(Weekday),
}

fn err(message: &str) -> DiagnosticKind {
    DiagnosticKind::Script(message.to_owned())
}
fn range() -> DiagnosticKind {
    err("calendar value is out of range")
}
fn valid_date(value: NaiveDate) -> Result<NaiveDate, DiagnosticKind> {
    if (0..=9999).contains(&value.year()) {
        Ok(value)
    } else {
        Err(range())
    }
}
fn offset_local(value: &DateTime<FixedOffset>) -> Result<NaiveDateTime, DiagnosticKind> {
    let local = value
        .naive_utc()
        .checked_add_signed(Duration::seconds(i64::from(
            value.offset().local_minus_utc(),
        )))
        .ok_or_else(range)?;
    valid_date(local.date())?;
    Ok(local)
}

fn local(instant: DateTime<Utc>, zone: NamedTimeZone) -> Result<NaiveDateTime, DiagnosticKind> {
    let tz: chrono_tz::Tz = zone.name().parse().map_err(|_| range())?;
    let projected = instant.with_timezone(&tz);
    use chrono::Offset;
    let local = instant
        .naive_utc()
        .checked_add_signed(Duration::seconds(i64::from(
            projected.offset().fix().local_minus_utc(),
        )))
        .ok_or_else(range)?;
    valid_date(local.date())?;
    Ok(local)
}

fn resolve(
    value: NaiveDateTime,
    zone: NamedTimeZone,
    fold: Option<FoldChoice>,
) -> Result<CalendarValue, DiagnosticKind> {
    valid_date(value.date())?;
    let resolved = resolve_local_datetime(value, zone)
        .and_then(|resolution| resolution.select(fold))
        .map_err(|e| err(&e.to_string()))?;
    Ok(CalendarValue::Zoned {
        instant: resolved.instant_utc,
        zone,
    })
}

fn fold(value: Option<&str>) -> Result<Option<FoldChoice>, DiagnosticKind> {
    match value {
        None => Ok(None),
        Some("earlier") => Ok(Some(FoldChoice::Earlier)),
        Some("later") => Ok(Some(FoldChoice::Later)),
        _ => Err(err("fold choice must be \"earlier\" or \"later\"")),
    }
}

impl CalendarValue {
    pub fn display(&self) -> String {
        match self {
            Self::Date(v) => v.format("%Y-%m-%d").to_string(),
            Self::Time(v) => v.format("%H:%M:%S%.f").to_string(),
            Self::Offset(v) => v.to_rfc3339(),
            Self::Zoned { instant, zone } => format!(
                "{} {}",
                local(*instant, *zone)
                    .map(|v| v.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
                    .unwrap_or_else(|_| "out of range".into()),
                zone.name()
            ),
            Self::Weekday(v) => format!("{v:?}"),
        }
    }
    pub fn canonical(&self) -> Option<String> {
        Some(match self {
            Self::Date(_) => format!("date(\"{}\")", self.display()),
            Self::Time(_) => format!("time(\"{}\")", self.display()),
            Self::Offset(_) => format!("datetime(\"{}\")", self.display()),
            Self::Zoned { instant, zone } => {
                let value = local(*instant, *zone).ok()?;
                let side = match resolve_local_datetime(value, *zone) {
                    Ok(LocalResolution::Ambiguous { earlier, .. })
                        if earlier.instant_utc == *instant =>
                    {
                        ", \"earlier\""
                    }
                    Ok(LocalResolution::Ambiguous { .. }) => ", \"later\"",
                    _ => "",
                };
                format!(
                    "zoned_datetime(\"{}\", \"{}\"{})",
                    value.format("%Y-%m-%dT%H:%M:%S%.f"),
                    zone.name(),
                    side
                )
            }
            Self::Weekday(v) => format!(
                "weekday(date(\"{}\"))",
                match v {
                    Weekday::Mon => "2026-11-16",
                    Weekday::Tue => "2026-11-17",
                    Weekday::Wed => "2026-11-18",
                    Weekday::Thu => "2026-11-19",
                    Weekday::Fri => "2026-11-20",
                    Weekday::Sat => "2026-11-21",
                    Weekday::Sun => "2026-11-15",
                }
            ),
        })
    }
    pub fn compare(&self, other: &Self) -> Result<Ordering, DiagnosticKind> {
        match (self, other) {
            (Self::Date(a), Self::Date(b)) => Ok(a.cmp(b)),
            (Self::Time(a), Self::Time(b)) => Ok(a.cmp(b)),
            (Self::Offset(a), Self::Offset(b)) => Ok(a.cmp(b)),
            (Self::Zoned { instant: a, .. }, Self::Zoned { instant: b, .. }) => Ok(a.cmp(b)),
            (Self::Offset(a), Self::Zoned { instant: b, .. }) => Ok(a.with_timezone(&Utc).cmp(b)),
            (Self::Zoned { instant: a, .. }, Self::Offset(b)) => Ok(a.cmp(&b.with_timezone(&Utc))),
            (Self::Weekday(a), Self::Weekday(b)) => {
                Ok(a.num_days_from_monday().cmp(&b.num_days_from_monday()))
            }
            _ => Err(err("calendar values have incompatible types")),
        }
    }
}

pub(super) fn construct(name: &str, args: &[String]) -> Result<CalculationValue, DiagnosticKind> {
    let bad = || err("invalid calendar literal");
    if args
        .iter()
        .any(|s| s.len() > 128 || s.chars().any(char::is_control))
    {
        return Err(bad());
    }
    if matches!(name, "time" | "datetime" | "zoned_datetime")
        && args.first().is_some_and(|value| {
            value.split_once('.').is_some_and(|(_, fraction)| {
                fraction.bytes().take_while(u8::is_ascii_digit).count() > 9
            })
        })
    {
        return Err(err("calendar time precision exceeds nanoseconds"));
    }
    let value = match (name, args) {
        ("date", [v]) => {
            let d = NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| bad())?;
            valid_date(d)?;
            if d.format("%Y-%m-%d").to_string() != *v {
                return Err(bad());
            }
            CalendarValue::Date(d)
        }
        ("time", [v]) => {
            let t = NaiveTime::parse_from_str(v, "%H:%M:%S%.f")
                .or_else(|_| NaiveTime::parse_from_str(v, "%H:%M"))
                .map_err(|_| bad())?;
            if t.nanosecond() >= 1_000_000_000 {
                return Err(bad());
            }
            CalendarValue::Time(t)
        }
        ("datetime", [v]) => {
            let d = DateTime::parse_from_rfc3339(v).map_err(|_| bad())?;
            let local = offset_local(&d)?;
            if local.time().nanosecond() >= 1_000_000_000 {
                return Err(bad());
            }
            CalendarValue::Offset(d)
        }
        ("zoned_datetime", [v, z] | [v, z, _]) => {
            let date = NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M:%S%.f")
                .or_else(|_| NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M"))
                .map_err(|_| bad())?;
            if date.time().nanosecond() >= 1_000_000_000 {
                return Err(bad());
            }
            let zone = NamedTimeZone::parse(z).map_err(|e| err(&e.to_string()))?;
            return resolve(date, zone, fold(args.get(2).map(String::as_str))?)
                .map(CalculationValue::Calendar);
        }
        _ => return Err(bad()),
    };
    Ok(CalculationValue::Calendar(value))
}

pub(super) fn call(
    name: &str,
    values: &[CalculationValue],
    fold_text: Option<&str>,
) -> Result<CalculationValue, DiagnosticKind> {
    let cv = |v: &CalculationValue| match v {
        CalculationValue::Calendar(c) => Ok(c.clone()),
        _ => Err(err("expected a calendar value")),
    };
    let choice = fold(fold_text)?;
    let result = match (name, values) {
        ("weekday", [v]) => match cv(v)? {
            CalendarValue::Date(d) => {
                CalculationValue::Calendar(CalendarValue::Weekday(d.weekday()))
            }
            _ => return Err(err("weekday requires a date")),
        },
        ("days_between", [a, b]) => match (cv(a)?, cv(b)?) {
            (CalendarValue::Date(a), CalendarValue::Date(b)) => {
                CalculationValue::Scalar(b.signed_duration_since(a).num_days() as f64)
            }
            _ => return Err(err("days_between requires two dates")),
        },
        ("add_days" | "add_months" | "add_years", [v, n]) => {
            use num_traits::ToPrimitive;
            let count = super::exact::ExactNumber::integer(n)?
                .to_i64()
                .ok_or_else(range)?;
            let shift = |d: NaiveDate| -> Result<NaiveDate, DiagnosticKind> {
                let shifted = if name == "add_days" {
                    d.checked_add_signed(Duration::try_days(count).ok_or_else(range)?)
                        .ok_or_else(range)?
                } else {
                    shift_calendar_months(
                        d,
                        if name == "add_years" {
                            count.checked_mul(12).ok_or_else(range)?
                        } else {
                            count
                        },
                    )
                    .ok_or_else(range)?
                };
                valid_date(shifted)
            };
            let value = match cv(v)? {
                CalendarValue::Date(d) => {
                    if choice.is_some() {
                        return Err(err("fold choice applies only to named-zone timestamps"));
                    }
                    CalendarValue::Date(shift(d)?)
                }
                CalendarValue::Offset(d) => {
                    if choice.is_some() {
                        return Err(err("fold choice applies only to named-zone timestamps"));
                    }
                    let local = offset_local(&d)?;
                    let shifted = shift(local.date())?.and_time(local.time());
                    let value = d
                        .offset()
                        .from_local_datetime(&shifted)
                        .single()
                        .ok_or_else(range)?;
                    offset_local(&value)?;
                    CalendarValue::Offset(value)
                }
                CalendarValue::Zoned { instant, zone } => {
                    let old = local(instant, zone)?;
                    resolve(shift(old.date())?.and_time(old.time()), zone, choice)?
                }
                _ => return Err(err("calendar shift requires a date or timestamp")),
            };
            CalculationValue::Calendar(value)
        }
        ("elapsed_between", [a, b]) => {
            let duration = match (cv(a)?, cv(b)?) {
                (CalendarValue::Time(a), CalendarValue::Time(b)) => b.signed_duration_since(a),
                (CalendarValue::Offset(a), CalendarValue::Offset(b)) => b.signed_duration_since(a),
                (
                    CalendarValue::Zoned { instant: a, .. },
                    CalendarValue::Zoned { instant: b, .. },
                ) => b.signed_duration_since(a),
                (CalendarValue::Offset(a), CalendarValue::Zoned { instant: b, .. }) => {
                    b.signed_duration_since(a.with_timezone(&Utc))
                }
                (CalendarValue::Zoned { instant: a, .. }, CalendarValue::Offset(b)) => {
                    b.with_timezone(&Utc).signed_duration_since(a)
                }
                _ => {
                    return Err(err(
                        "elapsed_between requires compatible times or timestamps",
                    ));
                }
            };
            let seconds = duration.num_seconds() as f64 + f64::from(duration.subsec_nanos()) / 1e9;
            CalculationValue::PhysicalQuantity {
                value: seconds,
                unit: Unit::Second,
                approximations: Vec::new(),
            }
        }
        ("add_duration", [v, q]) => {
            let seconds = match q {
                CalculationValue::PhysicalQuantity {
                    value,
                    unit,
                    approximations,
                } if approximations.is_empty() => {
                    unit_conversion::convert(*value, *unit, Unit::Second)
                }
                CalculationValue::DimensionalQuantity(q) if q.approximations().is_empty() => {
                    quantity::convert_to_unit(q, Unit::Second)
                        .ok()
                        .and_then(|v| v.display_parts().map(|x| x.0))
                }
                _ => None,
            }
            .filter(|x| x.is_finite())
            .ok_or_else(|| err("duration requires an unapproximated physical time quantity"))?;
            let whole = seconds.trunc();
            if whole < i64::MIN as f64 || whole >= i64::MAX as f64 {
                return Err(range());
            }
            let fractional = (seconds - whole) * 1e9;
            let nanos = fractional.round();
            // The source quantity is already an f64. Account for its rounding
            // before deciding whether it represents a whole nanosecond.
            let magnitude = seconds.abs();
            let ulp = f64::from_bits(magnitude.to_bits() + 1) - magnitude;
            let tolerance = (ulp * 0.5 * 1e9).max(1e-5);
            if (fractional - nanos).abs() > tolerance {
                return Err(err("duration precision exceeds nanoseconds"));
            }
            let duration = Duration::try_seconds(whole as i64)
                .and_then(|value| value.checked_add(&Duration::nanoseconds(nanos as i64)))
                .ok_or_else(range)?;
            let value = match cv(v)? {
                CalendarValue::Time(t) => {
                    let (t, overflow) = t.overflowing_add_signed(duration);
                    if overflow != 0 {
                        return Err(err("time crosses a day boundary; use a timestamp"));
                    }
                    CalendarValue::Time(t)
                }
                CalendarValue::Offset(t) => {
                    let t = t.checked_add_signed(duration).ok_or_else(range)?;
                    offset_local(&t)?;
                    CalendarValue::Offset(t)
                }
                CalendarValue::Zoned { instant, zone } => {
                    let instant = instant.checked_add_signed(duration).ok_or_else(range)?;
                    local(instant, zone)?;
                    CalendarValue::Zoned { instant, zone }
                }
                _ => return Err(err("duration addition requires a time or timestamp")),
            };
            CalculationValue::Calendar(value)
        }
        _ => return Err(err("invalid calendar arguments")),
    };
    Ok(result)
}
