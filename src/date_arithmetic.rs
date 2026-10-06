//! Deterministic date anchors for the date-arithmetic domain.
//!
//! Parsing receives its reference local date/time explicitly. This module
//! never reads the system clock or interprets time zones.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

/// An anchor retains whether its input represented a calendar date or a local
/// date-time so later arithmetic can apply the correct rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateValue {
    Date(NaiveDate),
    DateTime(NaiveDateTime),
}

/// Errors produced while parsing a concrete date or a date anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateAnchorError {
    InvalidDate { input: String },
    InvalidDateTime { input: String },
    UnknownAnchor { input: String },
    OutOfRange { input: String },
}

impl std::fmt::Display for DateAnchorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDate { input } => write!(formatter, "invalid date: {input}"),
            Self::InvalidDateTime { input } => {
                write!(formatter, "invalid local date-time: {input}")
            }
            Self::UnknownAnchor { input } => write!(formatter, "unknown date anchor: {input}"),
            Self::OutOfRange { input } => {
                write!(formatter, "date is outside the supported range: {input}")
            }
        }
    }
}

impl std::error::Error for DateAnchorError {}

/// Errors produced while parsing or applying a bounded relative date offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateArithmeticError {
    Anchor(DateAnchorError),
    InvalidSyntax { expression: String },
    UnknownUnit { unit: String, expression: String },
    TimeRequired { unit: String, expression: String },
    OutOfRange { expression: String },
}

impl std::fmt::Display for DateArithmeticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Anchor(error) => write!(formatter, "{error}"),
            Self::InvalidSyntax { expression } => {
                write!(
                    formatter,
                    "invalid date arithmetic expression: {expression}"
                )
            }
            Self::UnknownUnit { unit, expression } => {
                write!(
                    formatter,
                    "unknown date arithmetic unit '{unit}': {expression}"
                )
            }
            Self::TimeRequired { unit, expression } => write!(
                formatter,
                "a date-time anchor is required for {unit}: {expression}"
            ),
            Self::OutOfRange { expression } => {
                write!(
                    formatter,
                    "date arithmetic is outside the supported range: {expression}"
                )
            }
        }
    }
}

impl std::error::Error for DateArithmeticError {}

impl From<DateAnchorError> for DateArithmeticError {
    fn from(error: DateAnchorError) -> Self {
        Self::Anchor(error)
    }
}

/// Parses a concrete date or relative anchor against an explicit local
/// reference. Relative date anchors use `reference_now.date()`; `now` and
/// explicit local date-times retain their time component.
pub fn parse_anchor(
    input: &str,
    reference_now: NaiveDateTime,
) -> Result<DateValue, DateAnchorError> {
    let input = input.trim();
    let lower = input.to_lowercase();
    let unknown = || DateAnchorError::UnknownAnchor {
        input: input.to_owned(),
    };

    if input.is_empty() {
        return Err(unknown());
    }

    match lower.as_str() {
        "today" => return Ok(DateValue::Date(reference_now.date())),
        "tomorrow" => {
            return reference_now
                .date()
                .checked_add_signed(Duration::days(1))
                .map(DateValue::Date)
                .ok_or_else(|| anchor_out_of_range(input));
        }
        "yesterday" => {
            return reference_now
                .date()
                .checked_sub_signed(Duration::days(1))
                .map(DateValue::Date)
                .ok_or_else(|| anchor_out_of_range(input));
        }
        "now" => return Ok(DateValue::DateTime(reference_now)),
        _ => {}
    }

    if let Some(date_time) = parse_local_date_time(input) {
        return Ok(DateValue::DateTime(date_time));
    }
    if looks_like_date_time(input) {
        return Err(DateAnchorError::InvalidDateTime {
            input: input.to_owned(),
        });
    }

    if let Some(date) = parse_concrete_date(input) {
        return date.map(DateValue::Date);
    }
    if let Some(date) = parse_weekday_anchor(input, reference_now.date()) {
        return date.map(DateValue::Date);
    }
    if let Some(date) = parse_named_date(input, reference_now.year()) {
        return date.map(DateValue::Date);
    }

    Err(unknown())
}

/// Evaluates a bare anchor or one bounded relative date expression against an
/// explicit local reference. Supported forms are `3 days after today`,
/// `3 days before today`, `3 days from today`, and `today + 3 days`.
pub fn evaluate_expression(
    expression: &str,
    reference_now: NaiveDateTime,
) -> Result<DateValue, DateArithmeticError> {
    let Some(relative) = parse_relative_expression(expression) else {
        return parse_anchor(expression, reference_now).map_err(Into::into);
    };
    let relative = relative?;
    let anchor = parse_anchor(&relative.anchor, reference_now)?;
    apply_offset(anchor, relative.amount, relative.unit, expression)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OffsetUnit {
    Day,
    Week,
    Month,
    Year,
    Hour,
    Minute,
}

impl OffsetUnit {
    fn label(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
            Self::Hour => "hour",
            Self::Minute => "minute",
        }
    }

    fn is_subday(self) -> bool {
        matches!(self, Self::Hour | Self::Minute)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelativeOffset {
    anchor: String,
    amount: i64,
    unit: OffsetUnit,
}

fn parse_relative_expression(
    expression: &str,
) -> Option<Result<RelativeOffset, DateArithmeticError>> {
    let words: Vec<_> = expression.split_whitespace().collect();

    if words.len() >= 3 && is_direction(words[2]) {
        let direction = words[2];
        return Some(parse_relative_parts(
            expression,
            &words[3..],
            words.first().copied().unwrap_or_default(),
            words.get(1).copied().unwrap_or_default(),
            direction.eq_ignore_ascii_case("before"),
        ));
    }

    let operators: Vec<_> = words
        .iter()
        .enumerate()
        .filter(|(_, word)| **word == "+" || **word == "-")
        .collect();
    if operators.is_empty() {
        return None;
    }
    if operators.len() != 1 {
        return Some(Err(invalid_syntax(expression)));
    }

    let (operator_index, operator) = operators[0];
    if operator_index == 0 || operator_index + 3 != words.len() {
        return Some(Err(invalid_syntax(expression)));
    }
    Some(parse_relative_parts(
        expression,
        &words[..operator_index],
        words[operator_index + 1],
        words[operator_index + 2],
        *operator == "-",
    ))
}

fn is_direction(word: &str) -> bool {
    ["from", "after", "before"]
        .iter()
        .any(|direction| word.eq_ignore_ascii_case(direction))
}

fn parse_relative_parts(
    expression: &str,
    anchor_words: &[&str],
    amount_text: &str,
    unit_text: &str,
    reverse: bool,
) -> Result<RelativeOffset, DateArithmeticError> {
    if anchor_words.is_empty() || amount_text.is_empty() || unit_text.is_empty() {
        return Err(invalid_syntax(expression));
    }
    let mut amount = parse_amount(amount_text, expression)?;
    if reverse {
        amount = amount
            .checked_neg()
            .ok_or_else(|| out_of_range(expression))?;
    }
    let unit = parse_offset_unit(unit_text).ok_or_else(|| DateArithmeticError::UnknownUnit {
        unit: unit_text.to_owned(),
        expression: expression.to_owned(),
    })?;
    Ok(RelativeOffset {
        anchor: anchor_words.join(" "),
        amount,
        unit,
    })
}

fn parse_amount(amount: &str, expression: &str) -> Result<i64, DateArithmeticError> {
    match amount.parse::<i64>() {
        Ok(amount) => Ok(amount),
        Err(_) => {
            let digits = amount
                .strip_prefix('+')
                .or_else(|| amount.strip_prefix('-'))
                .unwrap_or(amount);
            if !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit()) {
                Err(out_of_range(expression))
            } else {
                Err(invalid_syntax(expression))
            }
        }
    }
}

fn parse_offset_unit(unit: &str) -> Option<OffsetUnit> {
    match unit.to_lowercase().as_str() {
        "day" | "days" => Some(OffsetUnit::Day),
        "week" | "weeks" => Some(OffsetUnit::Week),
        "month" | "months" => Some(OffsetUnit::Month),
        "year" | "years" => Some(OffsetUnit::Year),
        "hour" | "hours" => Some(OffsetUnit::Hour),
        "minute" | "minutes" | "min" | "mins" => Some(OffsetUnit::Minute),
        _ => None,
    }
}

fn apply_offset(
    anchor: DateValue,
    amount: i64,
    unit: OffsetUnit,
    expression: &str,
) -> Result<DateValue, DateArithmeticError> {
    if unit.is_subday() && matches!(anchor, DateValue::Date(_)) {
        return Err(DateArithmeticError::TimeRequired {
            unit: unit.label().to_owned(),
            expression: expression.to_owned(),
        });
    }

    if matches!(unit, OffsetUnit::Month | OffsetUnit::Year) {
        let months = if unit == OffsetUnit::Year {
            amount
                .checked_mul(12)
                .ok_or_else(|| out_of_range(expression))?
        } else {
            amount
        };
        return match anchor {
            DateValue::Date(date) => shift_calendar_months(date, months)
                .map(DateValue::Date)
                .ok_or_else(|| out_of_range(expression)),
            DateValue::DateTime(date_time) => {
                let date = shift_calendar_months(date_time.date(), months)
                    .ok_or_else(|| out_of_range(expression))?;
                Ok(DateValue::DateTime(NaiveDateTime::new(
                    date,
                    date_time.time(),
                )))
            }
        };
    }

    let duration = match unit {
        OffsetUnit::Day => Duration::try_days(amount),
        OffsetUnit::Week => Duration::try_weeks(amount),
        OffsetUnit::Hour => Duration::try_hours(amount),
        OffsetUnit::Minute => Duration::try_minutes(amount),
        OffsetUnit::Month | OffsetUnit::Year => unreachable!("calendar units handled above"),
    }
    .ok_or_else(|| out_of_range(expression))?;

    match anchor {
        DateValue::Date(date) => date
            .checked_add_signed(duration)
            .map(DateValue::Date)
            .ok_or_else(|| out_of_range(expression)),
        DateValue::DateTime(date_time) => date_time
            .checked_add_signed(duration)
            .map(DateValue::DateTime)
            .ok_or_else(|| out_of_range(expression)),
    }
}

fn shift_calendar_months(date: NaiveDate, months: i64) -> Option<NaiveDate> {
    let current_month = i64::from(date.year())
        .checked_mul(12)?
        .checked_add(i64::from(date.month0()))?;
    let target_month = current_month.checked_add(months)?;
    let year = i32::try_from(target_month.div_euclid(12)).ok()?;
    let month = u32::try_from(target_month.rem_euclid(12) + 1).ok()?;
    (1..=date.day())
        .rev()
        .find_map(|day| NaiveDate::from_ymd_opt(year, month, day))
}

fn invalid_syntax(expression: &str) -> DateArithmeticError {
    DateArithmeticError::InvalidSyntax {
        expression: expression.trim().to_owned(),
    }
}

fn out_of_range(expression: &str) -> DateArithmeticError {
    DateArithmeticError::OutOfRange {
        expression: expression.trim().to_owned(),
    }
}

fn anchor_out_of_range(input: &str) -> DateAnchorError {
    DateAnchorError::OutOfRange {
        input: input.to_owned(),
    }
}

fn parse_local_date_time(input: &str) -> Option<NaiveDateTime> {
    [
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ]
    .into_iter()
    .find_map(|format| NaiveDateTime::parse_from_str(input, format).ok())
}

fn looks_like_date_time(input: &str) -> bool {
    input
        .split_once(' ')
        .is_some_and(|(date, time)| date.contains('-') && time.contains(':'))
        || input
            .split_once('T')
            .or_else(|| input.split_once('t'))
            .is_some_and(|(date, time)| date.contains('-') && time.contains(':'))
}

fn parse_concrete_date(input: &str) -> Option<Result<NaiveDate, DateAnchorError>> {
    let parts: Vec<_> = input.split('-').collect();
    if parts.len() == 3
        && parts[0].len() == 4
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
    {
        let year = parts[0].parse::<i32>();
        let month = parts[1].parse::<u32>();
        let day = parts[2].parse::<u32>();
        return Some(match (year, month, day) {
            (Ok(year), Ok(month), Ok(day)) => make_date(year, month, day, input),
            _ => Err(invalid_date(input)),
        });
    }

    if input.contains('/') {
        let parts: Vec<_> = input.split('/').map(str::trim).collect();
        if parts.len() == 3 && parts[0].chars().all(|character| character.is_ascii_digit()) {
            let month = parts[0].parse::<u32>();
            let day = parts[1].parse::<u32>();
            let year = parts[2].parse::<i32>();
            return Some(match (month, day, year) {
                (Ok(month), Ok(day), Ok(year)) => make_date(year, month, day, input),
                _ => Err(invalid_date(input)),
            });
        }
    }

    parse_written_date(input)
}

fn parse_written_date(input: &str) -> Option<Result<NaiveDate, DateAnchorError>> {
    let parts: Vec<_> = input
        .split_whitespace()
        .map(|part| part.trim_matches(','))
        .collect();
    if parts.len() != 3 {
        return None;
    }

    if let Some(month) = parse_month(parts[0]) {
        let day = parts[1].parse::<u32>();
        let year = parts[2].parse::<i32>();
        return Some(match (day, year) {
            (Ok(day), Ok(year)) => make_date(year, month, day, input),
            _ => Err(invalid_date(input)),
        });
    }
    if let Some(month) = parse_month(parts[1]) {
        let day = parts[0].parse::<u32>();
        let year = parts[2].parse::<i32>();
        return Some(match (day, year) {
            (Ok(day), Ok(year)) => make_date(year, month, day, input),
            _ => Err(invalid_date(input)),
        });
    }

    None
}

fn parse_month(input: &str) -> Option<u32> {
    match input.to_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

fn make_date(year: i32, month: u32, day: u32, input: &str) -> Result<NaiveDate, DateAnchorError> {
    NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| invalid_date(input))
}

fn invalid_date(input: &str) -> DateAnchorError {
    DateAnchorError::InvalidDate {
        input: input.to_owned(),
    }
}

fn parse_weekday_anchor(
    input: &str,
    reference_date: NaiveDate,
) -> Option<Result<NaiveDate, DateAnchorError>> {
    let parts: Vec<_> = input.split_whitespace().collect();
    let (modifier, weekday) = match parts.as_slice() {
        [weekday] => (String::new(), *weekday),
        [modifier, weekday]
            if ["next", "last", "this"].contains(&modifier.to_lowercase().as_str()) =>
        {
            (modifier.to_lowercase(), *weekday)
        }
        _ => return None,
    };
    let target = parse_weekday(weekday)?;
    let current_index = i64::from(reference_date.weekday().num_days_from_monday());
    let target_index = i64::from(target.num_days_from_monday());

    let date = match modifier.as_str() {
        "" => {
            let offset = (target_index - current_index + 7) % 7;
            reference_date.checked_add_signed(Duration::days(offset))
        }
        "next" => {
            let mut offset = (target_index - current_index + 7) % 7;
            if offset == 0 {
                offset = 7;
            }
            reference_date.checked_add_signed(Duration::days(offset))
        }
        "last" => {
            let mut offset = (current_index - target_index + 7) % 7;
            if offset == 0 {
                offset = 7;
            }
            reference_date.checked_sub_signed(Duration::days(offset))
        }
        "this" => reference_date
            .checked_sub_signed(Duration::days(current_index))
            .and_then(|monday| monday.checked_add_signed(Duration::days(target_index))),
        _ => return None,
    };

    Some(date.ok_or_else(|| anchor_out_of_range(input)))
}

fn parse_weekday(input: &str) -> Option<Weekday> {
    match input.to_lowercase().as_str() {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn parse_named_date(
    input: &str,
    reference_year: i32,
) -> Option<Result<NaiveDate, DateAnchorError>> {
    let normalized = input
        .trim()
        .to_lowercase()
        .replace('’', "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let (name, year_text) = if let Some((name, year)) = normalized.rsplit_once(' ')
        && !year.is_empty()
        && year.chars().all(|character| character.is_ascii_digit())
    {
        (name, Some(year))
    } else if let Some((name, year)) = ["christmas", "new year's day", "new years day"]
        .into_iter()
        .find_map(|name| {
            normalized
                .strip_prefix(name)
                .filter(|year| !year.is_empty() && year.chars().all(|c| c.is_ascii_digit()))
                .map(|year| (name, year))
        })
    {
        (name, Some(year))
    } else {
        (normalized.as_str(), None)
    };
    let (month, day) = match name {
        "christmas" => (12, 25),
        "new year's day" | "new years day" => (1, 1),
        _ => return None,
    };
    let year = if let Some(year_text) = year_text {
        match year_text.parse::<i32>() {
            Ok(year) => year,
            Err(_) => return Some(Err(invalid_date(input))),
        }
    } else {
        reference_year
    };
    Some(make_date(year, month, day, input))
}

#[cfg(test)]
mod tests {
    use super::{
        DateAnchorError, DateArithmeticError, DateValue, evaluate_expression, parse_anchor,
    };
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

    fn fixed_reference() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 5)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap()
    }

    fn date(year: i32, month: u32, day: u32) -> DateValue {
        DateValue::Date(NaiveDate::from_ymd_opt(year, month, day).unwrap())
    }

    #[test]
    fn parses_iso_us_and_written_dates_case_insensitively() {
        let reference = fixed_reference();
        for input in [
            "2026-10-05",
            "10/5/2026",
            "10/05/2026",
            "October 5 2026",
            "October 5, 2026",
            "Oct 5 2026",
            "5 October 2026",
            "oCtObEr 5 2026",
        ] {
            assert_eq!(
                parse_anchor(input, reference),
                Ok(date(2026, 10, 5)),
                "{input}"
            );
        }
    }

    #[test]
    fn preserves_local_date_time_anchors() {
        let reference = fixed_reference();
        let expected = DateValue::DateTime(
            NaiveDate::from_ymd_opt(2026, 10, 5)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap(),
        );
        assert_eq!(parse_anchor("2026-10-05 14:30", reference), Ok(expected));
        assert_eq!(parse_anchor("2026-10-05T14:30", reference), Ok(expected));
        assert_eq!(
            parse_anchor("now", reference),
            Ok(DateValue::DateTime(reference))
        );
    }

    #[test]
    fn rejects_invalid_dates_and_accepts_only_valid_leap_days() {
        let reference = fixed_reference();
        for input in ["2026-02-30", "13/5/2026", "February 30 2026", "2025-02-29"] {
            assert!(
                matches!(
                    parse_anchor(input, reference),
                    Err(DateAnchorError::InvalidDate { .. })
                ),
                "{input} should be rejected as an invalid calendar date"
            );
        }
        assert_eq!(parse_anchor("2024-02-29", reference), Ok(date(2024, 2, 29)));
    }

    #[test]
    fn resolves_relative_day_anchors_from_the_supplied_reference() {
        let reference = fixed_reference();
        assert_eq!(parse_anchor("today", reference), Ok(date(2026, 10, 5)));
        assert_eq!(parse_anchor("tomorrow", reference), Ok(date(2026, 10, 6)));
        assert_eq!(parse_anchor("yesterday", reference), Ok(date(2026, 10, 4)));
        assert_eq!(
            parse_anchor("NOW", reference),
            Ok(DateValue::DateTime(reference))
        );
    }

    #[test]
    fn applies_bare_next_last_and_this_weekday_semantics() {
        let monday = fixed_reference();
        assert_eq!(parse_anchor("Monday", monday), Ok(date(2026, 10, 5)));
        assert_eq!(parse_anchor("Tuesday", monday), Ok(date(2026, 10, 6)));
        assert_eq!(parse_anchor("next Monday", monday), Ok(date(2026, 10, 12)));
        assert_eq!(parse_anchor("last Monday", monday), Ok(date(2026, 9, 28)));
        assert_eq!(parse_anchor("this Sunday", monday), Ok(date(2026, 10, 11)));

        let sunday = NaiveDate::from_ymd_opt(2026, 10, 11)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(14, 30, 0).unwrap());
        assert_eq!(parse_anchor("Sunday", sunday), Ok(date(2026, 10, 11)));
        assert_eq!(parse_anchor("next Sunday", sunday), Ok(date(2026, 10, 18)));
        assert_eq!(parse_anchor("last Sunday", sunday), Ok(date(2026, 10, 4)));
        assert_eq!(parse_anchor("this Monday", sunday), Ok(date(2026, 10, 5)));
    }

    #[test]
    fn resolves_named_fixed_dates_in_reference_or_explicit_year() {
        let reference = fixed_reference();
        assert_eq!(parse_anchor("Christmas", reference), Ok(date(2026, 12, 25)));
        assert_eq!(
            parse_anchor("Christmas 2027", reference),
            Ok(date(2027, 12, 25))
        );
        assert_eq!(
            parse_anchor("Christmas2027", reference),
            Ok(date(2027, 12, 25))
        );
        assert_eq!(
            parse_anchor("New Year's Day", reference),
            Ok(date(2026, 1, 1))
        );
        assert_eq!(
            parse_anchor("New Year's Day 2027", reference),
            Ok(date(2027, 1, 1))
        );
        assert_eq!(
            parse_anchor("New Year's Day2027", reference),
            Ok(date(2027, 1, 1))
        );
        assert!(matches!(
            parse_anchor("Christ-mas", reference),
            Err(DateAnchorError::UnknownAnchor { .. })
        ));
    }

    #[test]
    fn checked_date_bounds_and_unknown_anchors_return_typed_errors() {
        let max_reference = NaiveDate::MAX.and_hms_opt(23, 59, 59).unwrap();
        assert!(matches!(
            parse_anchor("tomorrow", max_reference),
            Err(DateAnchorError::OutOfRange { .. })
        ));
        assert!(matches!(
            parse_anchor("not a date", fixed_reference()),
            Err(DateAnchorError::UnknownAnchor { .. })
        ));
        assert!(matches!(
            parse_anchor("2026-10-05 25:00", fixed_reference()),
            Err(DateAnchorError::InvalidDateTime { .. })
        ));
    }

    #[test]
    fn evaluates_word_and_operator_offset_forms_from_a_fixed_reference() {
        let reference = fixed_reference();
        for (expression, expected) in [
            ("30 days from today", date(2026, 11, 4)),
            ("2 WEEKS from TOMORROW", date(2026, 10, 20)),
            ("3 months after 2026-10-05", date(2027, 1, 5)),
            ("10 days before Christmas", date(2026, 12, 15)),
            ("today + 10 days", date(2026, 10, 15)),
            ("Friday - 3 weeks", date(2026, 9, 18)),
            ("1 day from today", date(2026, 10, 6)),
            ("-3 days from today", date(2026, 10, 2)),
        ] {
            assert_eq!(
                evaluate_expression(expression, reference),
                Ok(expected),
                "{expression}"
            );
        }

        assert_eq!(
            evaluate_expression("2026-10-05", reference),
            Ok(date(2026, 10, 5)),
            "hyphens inside an ISO anchor are not offset operators"
        );
    }

    #[test]
    fn applies_calendar_month_and_year_offsets_with_clamping() {
        let reference = fixed_reference();
        for (expression, expected) in [
            ("1 month after 2025-01-31", date(2025, 2, 28)),
            ("1 month after 2024-01-31", date(2024, 2, 29)),
            ("2024-02-29 + 1 year", date(2025, 2, 28)),
            ("2024-02-29 - 1 year", date(2023, 2, 28)),
            ("3 months before 2026-10-05", date(2026, 7, 5)),
            ("2026-10-05 - 1 year", date(2025, 10, 5)),
            ("1 month after 2026-12-31", date(2027, 1, 31)),
        ] {
            assert_eq!(
                evaluate_expression(expression, reference),
                Ok(expected),
                "{expression}"
            );
        }
    }

    #[test]
    fn subday_offsets_require_and_preserve_a_date_time_anchor() {
        let reference = fixed_reference();
        let expected_from_now = DateValue::DateTime(
            NaiveDate::from_ymd_opt(2026, 10, 5)
                .unwrap()
                .and_hms_opt(17, 30, 0)
                .unwrap(),
        );
        let expected_explicit = DateValue::DateTime(
            NaiveDate::from_ymd_opt(2026, 10, 5)
                .unwrap()
                .and_hms_opt(16, 0, 0)
                .unwrap(),
        );
        assert_eq!(
            evaluate_expression("3 hours from now", reference),
            Ok(expected_from_now)
        );
        assert_eq!(
            evaluate_expression("90 minutes after 2026-10-05 14:30", reference),
            Ok(expected_explicit)
        );

        let expected_day_shift = DateValue::DateTime(
            NaiveDate::from_ymd_opt(2026, 10, 6)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap(),
        );
        let expected_month_shift = DateValue::DateTime(
            NaiveDate::from_ymd_opt(2025, 2, 28)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap(),
        );
        assert_eq!(
            evaluate_expression("1 day after 2026-10-05 14:30", reference),
            Ok(expected_day_shift)
        );
        assert_eq!(
            evaluate_expression("1 month after 2025-01-31 14:30", reference),
            Ok(expected_month_shift)
        );
        assert!(matches!(
            evaluate_expression("3 hours from today", reference),
            Err(DateArithmeticError::TimeRequired { .. })
        ));
    }

    #[test]
    fn offset_overflow_and_invalid_grammar_return_errors_without_panicking() {
        let reference = fixed_reference();
        for expression in [
            "9223372036854775807 days from today",
            "-9223372036854775808 days before today",
        ] {
            assert!(matches!(
                evaluate_expression(expression, reference),
                Err(DateArithmeticError::OutOfRange { .. })
            ));
        }
        assert!(matches!(
            evaluate_expression("today + 1 fortnight", reference),
            Err(DateArithmeticError::UnknownUnit { .. })
        ));
        assert!(matches!(
            evaluate_expression("today + 1", reference),
            Err(DateArithmeticError::InvalidSyntax { .. })
        ));
        assert!(matches!(
            evaluate_expression("two days from today", reference),
            Err(DateArithmeticError::InvalidSyntax { .. })
        ));
    }
}
