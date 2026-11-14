use crate::calculation_pad::CalculationValue;
use crate::settings::{
    CalculationPadDisplayPreferences as Preferences, CalculationPadNumberMode as Mode,
};

fn group(text: &str) -> String {
    let split = text.find(['.', 'e', 'E']).unwrap_or(text.len());
    let (integer, suffix) = text.split_at(split);
    let (sign, digits) = integer
        .strip_prefix('-')
        .map_or(("", integer), |rest| ("-", rest));
    let mut result = String::with_capacity(text.len() + digits.len() / 3);
    result.push_str(sign);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.push_str(suffix);
    result
}

fn scalar(value: f64, preferences: Preferences, percent: bool) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let value = if value == 0.0 { 0.0 } else { value };
    let percent = percent && preferences.percent;
    let scaled = if percent { value * 100.0 } else { value };
    let precision = usize::from(preferences.decimal_places.min(15));
    let mut text = if scaled.is_finite() {
        match preferences.number_mode {
            Mode::Auto => crate::common::number_format::format_number(scaled)?,
            Mode::Fixed => format!("{scaled:.precision$}"),
            Mode::Scientific => format!("{scaled:.precision$e}"),
        }
    } else {
        // The displayed percentage can exceed f64::MAX even though the stored
        // scalar is finite. Adjust the decimal exponent without overflowing.
        let digits = if preferences.number_mode == Mode::Auto {
            5
        } else {
            precision
        };
        let scientific = format!("{value:.digits$e}");
        let (mantissa, exponent) = scientific.split_once('e')?;
        let exponent = exponent.parse::<i32>().ok()?.checked_add(2)?;
        format!("{mantissa}e{exponent}")
    };
    if preferences.group_digits {
        text = group(&text);
    }
    if percent {
        text.push('%');
    }
    Some(text)
}

pub(super) fn supports_precision(value: &CalculationValue) -> bool {
    matches!(
        value,
        CalculationValue::Scalar(_)
            | CalculationValue::DimensionalQuantity(_)
            | CalculationValue::PhysicalQuantity { .. }
    )
}

pub(super) fn supports_percent(value: &CalculationValue) -> bool {
    match value {
        CalculationValue::Scalar(_) => true,
        CalculationValue::DimensionalQuantity(value) => value
            .display_parts()
            .is_some_and(|(_, unit)| unit.is_empty()),
        _ => false,
    }
}

pub(super) fn display(value: &CalculationValue, preferences: Preferences) -> Option<String> {
    // Auto is exactly the existing presentation when no additional option is set.
    if preferences.number_mode == Mode::Auto && !preferences.group_digits && !preferences.percent {
        return value.formatted_result();
    }
    match value {
        CalculationValue::Scalar(value) => scalar(*value, preferences, true),
        CalculationValue::Integer(value) => {
            let text = value.formatted_decimal();
            Some(if preferences.group_digits {
                group(&text)
            } else {
                text
            })
        }
        CalculationValue::DimensionalQuantity(value) => {
            let (number, unit) = value.display_parts()?;
            let text = scalar(number, preferences, unit.is_empty())?;
            Some(if unit.is_empty() {
                text
            } else {
                format!("{text} {unit}")
            })
        }
        CalculationValue::PhysicalQuantity { value, unit, .. } => Some(format!(
            "{} {}",
            scalar(*value, preferences, false)?,
            unit.symbol()
        )),
        _ => value.formatted_result(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formatting_is_local_bounded_and_preserves_exact_values() {
        let mut p = Preferences::default();
        let third = CalculationValue::Scalar(1.0 / 3.0);
        assert_eq!(display(&third, p), third.formatted_result());
        p.number_mode = Mode::Fixed;
        p.decimal_places = 3;
        assert_eq!(display(&third, p).as_deref(), Some("0.333"));
        p.group_digits = true;
        assert_eq!(
            display(&CalculationValue::Scalar(-1234.5), p).as_deref(),
            Some("-1,234.500")
        );
        p.percent = true;
        assert_eq!(display(&third, p).as_deref(), Some("33.333%"));
        assert_eq!(
            display(&CalculationValue::Scalar(-0.0), p).as_deref(),
            Some("0.000%")
        );
        let exact = CalculationValue::Integer(
            crate::calculation_pad::TypedInteger::from_unsigned(
                9_007_199_254_740_993,
                crate::calculation_pad::IntegerMode::unsigned(
                    crate::calculation_pad::IntegerWidth::Bits64,
                ),
            )
            .unwrap(),
        );
        assert_eq!(display(&exact, p).as_deref(), Some("9,007,199,254,740,993"));
        assert_eq!(
            exact.canonical_result().as_deref(),
            Some("9007199254740993")
        );
        assert_eq!(third.canonical_result().as_deref(), Some("0.333333"));
        assert_eq!(third, CalculationValue::Scalar(1.0 / 3.0));
        p.number_mode = Mode::Scientific;
        p.decimal_places = 2;
        assert_eq!(
            display(&CalculationValue::Scalar(1e200), p).as_deref(),
            Some("1.00e202%")
        );
        assert_eq!(
            display(&CalculationValue::Scalar(f64::MAX), p).as_deref(),
            Some("1.80e310%")
        );
    }
}
