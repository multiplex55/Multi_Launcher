//! Deterministic date anchors for the date-arithmetic domain.
//!
//! Parsing receives its reference local date/time explicitly. This module
//! never reads the system clock or interprets time zones.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};

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
    UnsupportedDifferenceUnit { unit: String, expression: String },
    IncompatibleDifferenceKinds { expression: String },
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
            Self::UnsupportedDifferenceUnit { unit, expression } => {
                write!(formatter, "differences do not support {unit}: {expression}")
            }
            Self::IncompatibleDifferenceKinds { expression } => write!(
                formatter,
                "difference operands must both be dates or both be date-times: {expression}"
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

/// Unit reported by a supported `between` expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateDifferenceUnit {
    Days,
    Weeks,
}

impl DateDifferenceUnit {
    fn label(self, value: f64) -> &'static str {
        match (self, value.abs() == 1.0) {
            (Self::Days, true) => "day",
            (Self::Days, false) => "days",
            (Self::Weeks, true) => "week",
            (Self::Weeks, false) => "weeks",
        }
    }
}

/// A typed date-domain result, separate from its human-readable and copied
/// representations.
#[derive(Debug, Clone, PartialEq)]
pub enum DateResult {
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    Difference {
        value: f64,
        unit: DateDifferenceUnit,
    },
}

/// Evaluation output ready for a launcher to display and copy without
/// re-parsing presentation strings.
#[derive(Debug, Clone, PartialEq)]
pub struct DateEvaluationOutcome {
    pub result: DateResult,
    pub display_label: String,
    pub clipboard_payload: String,
}

impl DateEvaluationOutcome {
    fn new(result: DateResult, expression: &str) -> Result<Self, DateArithmeticError> {
        let (display_label, clipboard_payload) = match &result {
            DateResult::Date(date) => {
                let iso = date.format("%Y-%m-%d").to_string();
                (format!("{} — {iso}", date.format("%A, %B %-d, %Y")), iso)
            }
            DateResult::DateTime(date_time) => {
                let iso = format!(
                    "{} {}",
                    date_time.date().format("%Y-%m-%d"),
                    format_local_time(date_time.time())
                );
                let display = format!(
                    "{} {} — {iso}",
                    date_time.date().format("%A, %B %-d, %Y"),
                    format_local_time(date_time.time())
                );
                (display, iso)
            }
            DateResult::Difference { value, unit } => {
                let number = crate::common::number_format::format_number(*value)
                    .ok_or_else(|| out_of_range(expression))?;
                let text = format!("{number} {}", unit.label(*value));
                (text.clone(), text)
            }
        };

        Ok(Self {
            result,
            display_label,
            clipboard_payload,
        })
    }
}

fn format_local_time(time: NaiveTime) -> String {
    if time.second() == 0 && time.nanosecond() == 0 {
        time.format("%H:%M").to_string()
    } else {
        let formatted = time.format("%H:%M:%S%.f").to_string();
        let Some((whole_seconds, fraction)) = formatted.split_once('.') else {
            return formatted;
        };
        let fraction = fraction.trim_end_matches('0');
        if fraction.is_empty() {
            whole_seconds.to_owned()
        } else {
            format!("{whole_seconds}.{fraction}")
        }
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
) -> Result<DateEvaluationOutcome, DateArithmeticError> {
    if let Some(difference) = parse_difference_expression(expression) {
        let difference = difference?;
        let first = parse_anchor(&difference.first, reference_now)?;
        let second = parse_anchor(&difference.second, reference_now)?;
        let days = difference_days(first, second, expression)?;
        let value = match difference.unit {
            DateDifferenceUnit::Days => days,
            DateDifferenceUnit::Weeks => days / 7.0,
        };
        if !value.is_finite() {
            return Err(out_of_range(expression));
        }
        return DateEvaluationOutcome::new(
            DateResult::Difference {
                value,
                unit: difference.unit,
            },
            expression,
        );
    }

    let result = match parse_relative_expression(expression) {
        Some(relative) => {
            let relative = relative?;
            let anchor = parse_anchor(&relative.anchor, reference_now)?;
            apply_offset(anchor, relative.amount, relative.unit, expression)?
        }
        None => parse_anchor(expression, reference_now)?,
    };
    let result = match result {
        DateValue::Date(date) => DateResult::Date(date),
        DateValue::DateTime(date_time) => DateResult::DateTime(date_time),
    };
    DateEvaluationOutcome::new(result, expression)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DifferenceExpression {
    first: String,
    second: String,
    unit: DateDifferenceUnit,
}

fn parse_difference_expression(
    expression: &str,
) -> Option<Result<DifferenceExpression, DateArithmeticError>> {
    let words: Vec<_> = expression.split_whitespace().collect();
    let between_positions: Vec<_> = words
        .iter()
        .enumerate()
        .filter(|(_, word)| word.eq_ignore_ascii_case("between"))
        .map(|(index, _)| index)
        .collect();
    if between_positions.is_empty() {
        return None;
    }

    Some((|| {
        if between_positions.len() != 1 || between_positions[0] != 1 || words.len() < 5 {
            return Err(invalid_syntax(expression));
        }
        let unit = match words[0].to_lowercase().as_str() {
            "day" | "days" => DateDifferenceUnit::Days,
            "week" | "weeks" => DateDifferenceUnit::Weeks,
            unsupported @ ("month" | "months" | "year" | "years") => {
                return Err(DateArithmeticError::UnsupportedDifferenceUnit {
                    unit: unsupported.to_owned(),
                    expression: expression.trim().to_owned(),
                });
            }
            unsupported => {
                return Err(DateArithmeticError::UnsupportedDifferenceUnit {
                    unit: unsupported.to_owned(),
                    expression: expression.trim().to_owned(),
                });
            }
        };

        let and_positions: Vec<_> = words
            .iter()
            .enumerate()
            .skip(2)
            .filter(|(_, word)| word.eq_ignore_ascii_case("and"))
            .map(|(index, _)| index)
            .collect();
        if and_positions.len() != 1 {
            return Err(invalid_syntax(expression));
        }
        let and_index = and_positions[0];
        if and_index == 2 || and_index + 1 == words.len() {
            return Err(invalid_syntax(expression));
        }

        Ok(DifferenceExpression {
            first: words[2..and_index].join(" "),
            second: words[and_index + 1..].join(" "),
            unit,
        })
    })())
}

fn difference_days(
    first: DateValue,
    second: DateValue,
    expression: &str,
) -> Result<f64, DateArithmeticError> {
    match (first, second) {
        (DateValue::Date(first), DateValue::Date(second)) => {
            Ok(second.signed_duration_since(first).num_days() as f64)
        }
        (DateValue::DateTime(first), DateValue::DateTime(second)) => {
            let duration = second.signed_duration_since(first);
            let seconds = duration.num_seconds() as f64
                + f64::from(duration.subsec_nanos()) / 1_000_000_000.0;
            Ok(seconds / 86_400.0)
        }
        _ => Err(DateArithmeticError::IncompatibleDifferenceKinds {
            expression: expression.trim().to_owned(),
        }),
    }
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
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
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
        DateAnchorError, DateArithmeticError, DateDifferenceUnit, DateResult, DateValue,
        evaluate_expression, parse_anchor,
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

    fn result_date(year: i32, month: u32, day: u32) -> DateResult {
        DateResult::Date(NaiveDate::from_ymd_opt(year, month, day).unwrap())
    }

    fn evaluated_result(
        expression: &str,
        reference: NaiveDateTime,
    ) -> Result<DateResult, DateArithmeticError> {
        evaluate_expression(expression, reference).map(|outcome| outcome.result)
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
            ("30 days from today", result_date(2026, 11, 4)),
            ("2 WEEKS from TOMORROW", result_date(2026, 10, 20)),
            ("3 months after 2026-10-05", result_date(2027, 1, 5)),
            ("10 days before Christmas", result_date(2026, 12, 15)),
            ("today + 10 days", result_date(2026, 10, 15)),
            ("Friday - 3 weeks", result_date(2026, 9, 18)),
            ("1 day from today", result_date(2026, 10, 6)),
            ("-3 days from today", result_date(2026, 10, 2)),
        ] {
            assert_eq!(
                evaluated_result(expression, reference),
                Ok(expected),
                "{expression}"
            );
        }

        assert_eq!(
            evaluated_result("2026-10-05", reference),
            Ok(result_date(2026, 10, 5)),
            "hyphens inside an ISO anchor are not offset operators"
        );
    }

    #[test]
    fn applies_calendar_month_and_year_offsets_with_clamping() {
        let reference = fixed_reference();
        for (expression, expected) in [
            ("1 month after 2025-01-31", result_date(2025, 2, 28)),
            ("1 month after 2024-01-31", result_date(2024, 2, 29)),
            ("2024-02-29 + 1 year", result_date(2025, 2, 28)),
            ("2024-02-29 - 1 year", result_date(2023, 2, 28)),
            ("3 months before 2026-10-05", result_date(2026, 7, 5)),
            ("2026-10-05 - 1 year", result_date(2025, 10, 5)),
            ("1 month after 2026-12-31", result_date(2027, 1, 31)),
        ] {
            assert_eq!(
                evaluated_result(expression, reference),
                Ok(expected),
                "{expression}"
            );
        }
    }

    #[test]
    fn subday_offsets_require_and_preserve_a_date_time_anchor() {
        let reference = fixed_reference();
        let expected_from_now = DateResult::DateTime(
            NaiveDate::from_ymd_opt(2026, 10, 5)
                .unwrap()
                .and_hms_opt(17, 30, 0)
                .unwrap(),
        );
        let expected_explicit = DateResult::DateTime(
            NaiveDate::from_ymd_opt(2026, 10, 5)
                .unwrap()
                .and_hms_opt(16, 0, 0)
                .unwrap(),
        );
        assert_eq!(
            evaluated_result("3 hours from now", reference),
            Ok(expected_from_now)
        );
        assert_eq!(
            evaluated_result("90 minutes after 2026-10-05 14:30", reference),
            Ok(expected_explicit)
        );

        let expected_day_shift = DateResult::DateTime(
            NaiveDate::from_ymd_opt(2026, 10, 6)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap(),
        );
        let expected_month_shift = DateResult::DateTime(
            NaiveDate::from_ymd_opt(2025, 2, 28)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap(),
        );
        assert_eq!(
            evaluated_result("1 day after 2026-10-05 14:30", reference),
            Ok(expected_day_shift)
        );
        assert_eq!(
            evaluated_result("1 month after 2025-01-31 14:30", reference),
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

    #[test]
    fn calculates_signed_day_and_week_differences_from_written_anchors() {
        let reference = fixed_reference();
        for (expression, expected) in [
            (
                "days between October 5 2026 and Christmas",
                DateResult::Difference {
                    value: 81.0,
                    unit: DateDifferenceUnit::Days,
                },
            ),
            (
                "DAYS BETWEEN Christmas and Oct 5 2026",
                DateResult::Difference {
                    value: -81.0,
                    unit: DateDifferenceUnit::Days,
                },
            ),
            (
                "weeks between 2026-10-05 and 2026-10-19",
                DateResult::Difference {
                    value: 2.0,
                    unit: DateDifferenceUnit::Weeks,
                },
            ),
            (
                "weeks between Oct 5 2026 and Jan 1 2027",
                DateResult::Difference {
                    value: 88.0 / 7.0,
                    unit: DateDifferenceUnit::Weeks,
                },
            ),
        ] {
            assert_eq!(
                evaluated_result(expression, reference),
                Ok(expected),
                "{expression}"
            );
        }

        let fractional_weeks =
            evaluate_expression("weeks between Oct 5 2026 and Jan 1 2027", reference).unwrap();
        assert_eq!(fractional_weeks.display_label, "12.5714 weeks");
        assert_eq!(fractional_weeks.clipboard_payload, "12.5714 weeks");
    }

    #[test]
    fn date_time_differences_preserve_fractional_days_and_reject_mixed_kinds() {
        let reference = fixed_reference();
        assert_eq!(
            evaluated_result(
                "days between 2026-10-05 14:30 and 2026-10-07 02:30",
                reference,
            ),
            Ok(DateResult::Difference {
                value: 1.5,
                unit: DateDifferenceUnit::Days,
            })
        );

        for (expression, expected_days) in [
            (
                "days between 2026-10-05 14:30:60 and 2026-10-05 14:31:00",
                1.0 / 86_400.0,
            ),
            (
                "days between 2026-10-05 14:31:00 and 2026-10-05 14:30:60",
                -1.0 / 86_400.0,
            ),
            (
                "days between 2026-10-05 14:30:00.000000001 and 2026-10-05 14:30:00",
                -1e-9 / 86_400.0,
            ),
        ] {
            assert_eq!(
                evaluated_result(expression, reference),
                Ok(DateResult::Difference {
                    value: expected_days,
                    unit: DateDifferenceUnit::Days,
                }),
                "{expression}"
            );
        }

        assert!(matches!(
            evaluate_expression("days between today and now", reference),
            Err(DateArithmeticError::IncompatibleDifferenceKinds { .. })
        ));
    }

    #[test]
    fn rejects_malformed_between_expressions_and_unsupported_difference_units() {
        let reference = fixed_reference();
        for expression in [
            "days between today",
            "days between today tomorrow",
            "days between today and",
            "days between and tomorrow",
            "days between today and tomorrow and Friday",
            "days between today and tomorrow and",
        ] {
            assert!(
                matches!(
                    evaluate_expression(expression, reference),
                    Err(DateArithmeticError::InvalidSyntax { .. })
                ),
                "{expression}"
            );
        }
        assert!(matches!(
            evaluate_expression("months between today and Christmas", reference),
            Err(DateArithmeticError::UnsupportedDifferenceUnit { .. })
        ));
    }

    #[test]
    fn date_results_have_human_iso_display_and_separate_clipboard_payloads() {
        let reference = fixed_reference();
        let date_outcome = evaluate_expression("30 days from today", reference).unwrap();
        assert_eq!(date_outcome.result, result_date(2026, 11, 4));
        assert_eq!(
            date_outcome.display_label,
            "Wednesday, November 4, 2026 — 2026-11-04"
        );
        assert_eq!(date_outcome.clipboard_payload, "2026-11-04");

        let minute_outcome = evaluate_expression("3 hours from now", reference).unwrap();
        assert_eq!(
            minute_outcome.display_label,
            "Monday, October 5, 2026 17:30 — 2026-10-05 17:30"
        );
        assert_eq!(minute_outcome.clipboard_payload, "2026-10-05 17:30");

        let seconds_outcome = evaluate_expression("2026-10-05T14:30:12.345600", reference).unwrap();
        assert_eq!(
            seconds_outcome.display_label,
            "Monday, October 5, 2026 14:30:12.3456 — 2026-10-05 14:30:12.3456"
        );
        assert_eq!(
            seconds_outcome.clipboard_payload,
            "2026-10-05 14:30:12.3456"
        );

        let leap_second_outcome = evaluate_expression("2026-10-05T14:30:60", reference).unwrap();
        assert_eq!(leap_second_outcome.clipboard_payload, "2026-10-05 14:30:60");
    }
}
