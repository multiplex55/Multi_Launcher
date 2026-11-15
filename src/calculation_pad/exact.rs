//! Bounded exact decimal and rational values for the calculation language.

use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{FromPrimitive, One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;

use super::document::{CalculationValue, DiagnosticKind, MAX_EXPRESSION_BYTES};
use super::document::script::WorkBudget;

const MAX_BITS: u64 = 4096;
const MAX_SCALE: u32 = 1000;

fn error(message: &str) -> DiagnosticKind {
    DiagnosticKind::Script(message.to_owned())
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExactNumber {
    value: BigRational,
    decimal: bool,
    rounded: bool,
}

impl ExactNumber {
    fn checked(value: BigRational, decimal: bool, rounded: bool, work: &mut WorkBudget) -> Result<Self, DiagnosticKind> {
        if value.numer().bits() > MAX_BITS || value.denom().bits() > MAX_BITS {
            return Err(error("exact number exceeds the 4096-bit limit"));
        }
        if decimal {
            let mut denominator = value.denom().clone();
            let mut twos = 0u64;
            let mut fives = 0u64;
            while denominator.is_even() { work.charge(1)?; denominator /= 2; twos += 1; }
            while (&denominator % 5u8).is_zero() { work.charge(1)?; denominator /= 5; fives += 1; }
            let scale = twos.max(fives);
            if denominator != BigInt::one() || scale > MAX_SCALE as u64
                || value.numer().bits().saturating_add(scale - twos).saturating_add((scale - fives) * 3) > MAX_BITS
            {
                return Err(error("decimal coefficient or scale exceeds the exact limit"));
            }
        }
        let result = Self { value, decimal, rounded };
        if result.canonical().len() > MAX_EXPRESSION_BYTES {
            return Err(error("exact number exceeds the canonical expression length limit"));
        }
        Ok(result)
    }

    pub(crate) fn decimal(text: &str, work: &mut WorkBudget) -> Result<Self, DiagnosticKind> {
        work.charge(text.len().max(1))?;
        let (negative, text) = if let Some(text) = text.strip_prefix('-') {
            (true, text)
        } else {
            (false, text.strip_prefix('+').unwrap_or(text))
        };
        let (mantissa, exponent) = match text.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().map_err(|_| error("invalid decimal exponent"))?),
            None => (text, 0),
        };
        let mut digits = String::with_capacity(mantissa.len());
        let mut scale = 0i32;
        let mut point = false;
        for ch in mantissa.chars() {
            if ch == '.' && !point {
                point = true;
            } else if ch.is_ascii_digit() {
                digits.push(ch);
                if point { scale += 1; }
            } else {
                return Err(error("invalid decimal literal"));
            }
        }
        if digits.is_empty() || digits.len() > 1234 {
            return Err(error("decimal coefficient exceeds the 4096-bit limit"));
        }
        scale = scale.checked_sub(exponent).ok_or_else(|| error("decimal scale exceeds the 1000-digit limit"))?;
        if scale.unsigned_abs() > MAX_SCALE {
            return Err(error("decimal scale exceeds the 1000-digit limit"));
        }
        let mut coefficient = BigInt::parse_bytes(digits.as_bytes(), 10).ok_or_else(|| error("invalid decimal literal"))?;
        if coefficient.bits() > MAX_BITS { return Err(error("decimal coefficient exceeds the 4096-bit limit")); }
        if negative { coefficient = -coefficient; }
        if scale < 0 {
            if coefficient.bits().saturating_add(((-scale) as u64) * 4) > MAX_BITS {
                return Err(error("decimal coefficient exceeds the 4096-bit limit"));
            }
            coefficient *= BigInt::from(10u8).pow((-scale) as u32);
            scale = 0;
        }
        Self::checked(BigRational::new(coefficient, BigInt::from(10u8).pow(scale as u32)), true, false, work)
    }

    pub(crate) fn from_integer_text(text: &str, work: &mut WorkBudget) -> Result<BigInt, DiagnosticKind> {
        work.charge(text.len().max(1))?;
        if text.len() > 1235 || text.is_empty() || !text.trim_start_matches(['+', '-']).bytes().all(|b| b.is_ascii_digit()) {
            return Err(error("fraction arguments must be bounded integers"));
        }
        let value = text.parse::<BigInt>().map_err(|_| error("invalid fraction integer"))?;
        if value.bits() > MAX_BITS { return Err(error("exact integer exceeds the 4096-bit limit")); }
        Ok(value)
    }

    pub(crate) fn fraction(numerator: BigInt, denominator: BigInt, work: &mut WorkBudget) -> Result<Self, DiagnosticKind> {
        work.charge((numerator.bits() + denominator.bits()) as usize / 64 + 1)?;
        if denominator.is_zero() { return Err(error("fraction denominator cannot be zero")); }
        Self::checked(BigRational::new(numerator, denominator), false, false, work)
    }

    pub(crate) fn integer(value: &CalculationValue) -> Result<BigInt, DiagnosticKind> {
        match value {
            CalculationValue::Integer(value) => match value.as_i128() {
                Some(value) => Ok(BigInt::from(value)),
                None => Ok(BigInt::from(value.bit_pattern())),
            },
            CalculationValue::ExactBase { value, .. } => Ok(BigInt::from(*value)),
            CalculationValue::Scalar(value) if value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_992.0 => {
                BigInt::from_f64(*value).ok_or_else(|| error("exact conversion requires an integral scalar"))
            }
            CalculationValue::Exact(value) if value.value.is_integer() => Ok(value.value.to_integer()),
            _ => Err(error("exact arithmetic requires an exact value or a finite integral scalar; use an explicit conversion")),
        }
    }

    fn rational(value: &CalculationValue) -> Result<(BigRational, bool, bool), DiagnosticKind> {
        match value {
            CalculationValue::Exact(value) => Ok((value.value.clone(), value.decimal, value.rounded)),
            _ => Ok((BigRational::from_integer(Self::integer(value)?), true, false)),
        }
    }

    pub(crate) fn arithmetic(left: &CalculationValue, right: &CalculationValue, operator: char, work: &mut WorkBudget) -> Result<CalculationValue, DiagnosticKind> {
        let (left, left_decimal, left_rounded) = Self::rational(left)?;
        let (right, right_decimal, right_rounded) = Self::rational(right)?;
        let growth = left.numer().bits().max(left.denom().bits()).saturating_add(right.numer().bits().max(right.denom().bits()));
        work.charge((growth / 32 + 1) as usize)?;
        if growth > MAX_BITS + 1 { return Err(error("exact arithmetic intermediate exceeds the 4096-bit limit")); }
        if operator == '/' && right.is_zero() { return Err(error("exact division by zero")); }
        let value = match operator {
            '+' => left + right,
            '-' => left - right,
            '*' => left * right,
            '/' => left / right,
            _ => return Err(error("operation is unsupported for exact values")),
        };
        let decimal = left_decimal && right_decimal;
        if decimal && Self::finite_scale(value.denom(), work)?.is_none() {
            return Err(error("decimal division is nonterminating; use decimal_div"));
        }
        Ok(CalculationValue::Exact(Self::checked(value, decimal, left_rounded || right_rounded, work)?))
    }

    fn finite_scale(denominator: &BigInt, work: &mut WorkBudget) -> Result<Option<u32>, DiagnosticKind> {
        let mut remaining = denominator.clone();
        let mut twos = 0;
        let mut fives = 0;
        while remaining.is_even() { work.charge(1)?; remaining /= 2; twos += 1; }
        while (&remaining % 5u8).is_zero() { work.charge(1)?; remaining /= 5; fives += 1; }
        let scale = twos.max(fives);
        if remaining != BigInt::one() || scale > MAX_SCALE { Ok(None) } else { Ok(Some(scale)) }
    }

    pub(crate) fn rounded_divide(left: &CalculationValue, right: &CalculationValue, places: u32, work: &mut WorkBudget) -> Result<Self, DiagnosticKind> {
        if places > MAX_SCALE { return Err(error("decimal precision exceeds the 1000-digit limit")); }
        let (left, _, left_rounded) = Self::rational(left)?;
        let (right, _, right_rounded) = Self::rational(right)?;
        if right.is_zero() { return Err(error("decimal division by zero")); }
        let growth = left.numer().bits() + right.denom().bits() + (places as u64 * 4);
        let denominator_growth = left.denom().bits() + right.numer().bits();
        work.charge((growth / 32 + 1) as usize)?;
        if growth > MAX_BITS || denominator_growth > MAX_BITS { return Err(error("rounded division intermediate exceeds the 4096-bit limit")); }
        let quotient = left / right;
        let factor = BigInt::from(10u8).pow(places);
        let scaled = quotient.numer() * &factor;
        let (mut integer, remainder) = scaled.div_rem(quotient.denom());
        let twice = remainder.abs() * 2u8;
        let comparison = twice.cmp(quotient.denom());
        if comparison == Ordering::Greater || (comparison == Ordering::Equal && integer.is_odd()) {
            integer += if scaled.sign() == Sign::Minus { -1 } else { 1 };
        }
        Self::checked(BigRational::new(integer, factor), true, left_rounded || right_rounded || !remainder.is_zero(), work)
    }

    pub(crate) fn compare(left: &CalculationValue, right: &CalculationValue, work: &mut WorkBudget) -> Result<Ordering, DiagnosticKind> {
        let (left, _, _) = Self::rational(left)?;
        let (right, _, _) = Self::rational(right)?;
        work.charge(((left.numer().bits() + left.denom().bits() + right.numer().bits() + right.denom().bits()) / 32 + 1) as usize)?;
        Ok(left.cmp(&right))
    }

    pub(crate) fn approximate(&self) -> Result<f64, DiagnosticKind> {
        self.value.to_f64().filter(|value| value.is_finite()).ok_or_else(|| error("exact value cannot be represented as a finite float"))
    }

    pub(crate) fn is_rounded(&self) -> bool { self.rounded }

    pub(crate) fn display(&self) -> String {
        let canonical = self.canonical();
        let value = if self.decimal {
            canonical.strip_prefix("decimal(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or(&canonical).to_owned()
        } else {
            format!("{}/{}", self.value.numer(), self.value.denom())
        };
        if self.rounded { format!("≈ {value}") } else { value }
    }

    pub(crate) fn canonical(&self) -> String {
        if self.decimal {
            let mut denominator = self.value.denom().clone();
            let mut twos = 0u32;
            let mut fives = 0u32;
            while denominator.is_even() { denominator /= 2; twos += 1; }
            while (&denominator % 5u8).is_zero() { denominator /= 5; fives += 1; }
            let scale = twos.max(fives);
            if denominator == BigInt::one() && scale <= MAX_SCALE {
                let coefficient = self.value.numer() * BigInt::from(2u8).pow(scale - twos) * BigInt::from(5u8).pow(scale - fives);
                let negative = coefficient.is_negative();
                let mut digits = coefficient.abs().to_string();
                if scale > 0 {
                    let needed = scale as usize + 1;
                    if digits.len() < needed { digits = format!("{}{}", "0".repeat(needed - digits.len()), digits); }
                    digits.insert(digits.len() - scale as usize, '.');
                }
                return format!("decimal(\"{}{}\")", if negative { "-" } else { "" }, digits);
            }
        }
        format!("fraction({},{})", self.value.numer(), self.value.denom())
    }
}
