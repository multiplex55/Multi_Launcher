//! Exact unsigned integer conversion for the supported power-of-two bases and decimal.

/// A supported radix for exact unsigned integer conversions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumericBase {
    Binary,
    Octal,
    Decimal,
    Hexadecimal,
}

impl NumericBase {
    /// Parses the aliases shared by the base converter and Calculation Pad.
    pub fn from_alias(alias: &str) -> Option<Self> {
        if alias.eq_ignore_ascii_case("bin") || alias.eq_ignore_ascii_case("binary") {
            Some(Self::Binary)
        } else if alias.eq_ignore_ascii_case("oct") || alias.eq_ignore_ascii_case("octal") {
            Some(Self::Octal)
        } else if alias.eq_ignore_ascii_case("dec") || alias.eq_ignore_ascii_case("decimal") {
            Some(Self::Decimal)
        } else if alias.eq_ignore_ascii_case("hex") || alias.eq_ignore_ascii_case("hexadecimal") {
            Some(Self::Hexadecimal)
        } else {
            None
        }
    }

    const fn radix(self) -> u32 {
        match self {
            Self::Binary => 2,
            Self::Octal => 8,
            Self::Decimal => 10,
            Self::Hexadecimal => 16,
        }
    }
}

/// Parses an unsigned integer without passing through floating point.
pub fn parse_unsigned(value: &str, base: NumericBase) -> Option<u128> {
    u128::from_str_radix(value, base.radix()).ok()
}

/// Formats an unsigned integer using lowercase digits and no prefix or padding.
pub fn format_unsigned(value: u128, base: NumericBase) -> String {
    match base {
        NumericBase::Binary => format!("{value:b}"),
        NumericBase::Octal => format!("{value:o}"),
        NumericBase::Decimal => value.to_string(),
        NumericBase::Hexadecimal => format!("{value:x}"),
    }
}

/// Converts an unsigned integer exactly between supported radices.
pub fn convert_unsigned(value: &str, from: NumericBase, to: NumericBase) -> Option<String> {
    parse_unsigned(value, from).map(|parsed| format_unsigned(parsed, to))
}

#[cfg(test)]
mod tests {
    use super::{NumericBase, convert_unsigned, format_unsigned, parse_unsigned};

    #[test]
    fn aliases_and_exact_radix_conversions_are_stable() {
        assert_eq!(NumericBase::from_alias("BIN"), Some(NumericBase::Binary));
        assert_eq!(NumericBase::from_alias("octal"), Some(NumericBase::Octal));
        assert_eq!(
            NumericBase::from_alias("decimal"),
            Some(NumericBase::Decimal)
        );
        assert_eq!(
            NumericBase::from_alias("Hexadecimal"),
            Some(NumericBase::Hexadecimal)
        );
        assert_eq!(NumericBase::from_alias("base16"), None);

        assert_eq!(
            convert_unsigned("1010", NumericBase::Binary, NumericBase::Hexadecimal),
            Some("a".into())
        );
        assert_eq!(
            convert_unsigned("ff", NumericBase::Hexadecimal, NumericBase::Binary),
            Some("11111111".into())
        );
        assert_eq!(
            convert_unsigned("17", NumericBase::Octal, NumericBase::Decimal),
            Some("15".into())
        );
        assert_eq!(
            convert_unsigned("15", NumericBase::Decimal, NumericBase::Octal),
            Some("17".into())
        );
    }

    #[test]
    fn preserves_unsigned_values_beyond_f64_integer_precision() {
        let value = 9_007_199_254_740_993_u128;
        let decimal = format_unsigned(value, NumericBase::Decimal);
        assert_eq!(decimal, "9007199254740993");
        assert_eq!(parse_unsigned(&decimal, NumericBase::Decimal), Some(value));

        let maximum = u128::MAX;
        for base in [
            NumericBase::Binary,
            NumericBase::Octal,
            NumericBase::Decimal,
            NumericBase::Hexadecimal,
        ] {
            let digits = format_unsigned(maximum, base);
            assert_eq!(parse_unsigned(&digits, base), Some(maximum));
        }
    }

    #[test]
    fn rejects_invalid_digits_and_unsigned_overflow() {
        assert_eq!(parse_unsigned("2", NumericBase::Binary), None);
        assert_eq!(parse_unsigned("g", NumericBase::Hexadecimal), None);
        assert_eq!(
            parse_unsigned(
                "340282366920938463463374607431768211456",
                NumericBase::Decimal
            ),
            None
        );
    }
}
