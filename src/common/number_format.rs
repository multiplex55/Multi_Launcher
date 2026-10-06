//! Shared deterministic formatting for finite numeric results.

/// Formats a finite number with up to six significant digits, removing
/// insignificant decimal zeroes and floating-point noise. Non-finite values
/// are rejected so callers can present an out-of-range error instead.
pub fn format_number(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some("0".to_owned());
    }

    let magnitude = value.abs();
    if magnitude >= 1e9 || magnitude < 1e-5 {
        let scientific = format!("{value:.5e}");
        let (mantissa, exponent) = scientific.split_once('e')?;
        let trimmed_mantissa = trim_fractional_zeroes(mantissa.to_owned());
        let exponent_value: i32 = exponent.parse().ok()?;
        return Some(format!("{trimmed_mantissa}e{exponent_value}"));
    }

    let nearest_integer = value.round();
    let integer_noise = (value.abs() * 1e-12)
        .max(f64::EPSILON * value.abs() * 4.0)
        .min(1e-6);
    if nearest_integer != 0.0 && (value - nearest_integer).abs() <= integer_noise {
        return Some(format!("{nearest_integer:.0}"));
    }

    let exponent = magnitude.log10().floor() as i32;
    let decimals = (5 - exponent).max(0) as usize;
    Some(trim_fractional_zeroes(format!("{value:.decimals$}")))
}

fn trim_fractional_zeroes(mut value: String) -> String {
    if value.contains('.') {
        while value.ends_with('0') {
            value.pop();
        }
        if value.ends_with('.') {
            value.pop();
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::format_number;

    #[test]
    fn removes_unneeded_zeroes_and_near_integer_noise() {
        assert_eq!(format_number(1.0), Some("1".to_owned()));
        assert_eq!(
            format_number(12.500_000_000_000_002),
            Some("12.5".to_owned())
        );
        assert_eq!(format_number(1.000_000_000_000_000_2), Some("1".to_owned()));
        assert_eq!(format_number(10.0 / 3.0), Some("3.33333".to_owned()));
    }

    #[test]
    fn keeps_small_nonzero_results_and_uses_scientific_notation_at_extremes() {
        assert_eq!(format_number(1.234_567e-30), Some("1.23457e-30".to_owned()));
        assert_eq!(format_number(1e200), Some("1e200".to_owned()));
        assert_eq!(format_number(1.234_567e12), Some("1.23457e12".to_owned()));
        assert_ne!(format_number(1e-300), Some("0".to_owned()));
    }

    #[test]
    fn rejects_non_finite_values() {
        assert_eq!(format_number(f64::NAN), None);
        assert_eq!(format_number(f64::INFINITY), None);
        assert_eq!(format_number(f64::NEG_INFINITY), None);
    }
}
