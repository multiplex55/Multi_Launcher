//! Bounded exact decimal and rational values for the calculation language.

use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{FromPrimitive, One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;

use super::document::{CalculationValue, DiagnosticKind, MAX_EXPRESSION_BYTES};
use super::document::script::WorkBudget;

pub(crate) const MAX_BITS: u64 = 4096;
pub(crate) const MAX_SCALE: u32 = 1000;
const MAX_COMBINATORIAL_STEPS: u64 = 10_000;

#[derive(Clone, Copy)]
pub(crate) enum EverydayFunction {
    RoundTo, FloorTo, CeilTo, Clamp, PercentChange, PercentOf,
    Choose, Permutations, Lerp, MapRange,
}

fn error(message: &str) -> DiagnosticKind {
    DiagnosticKind::Script(message.to_owned())
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExactNumber {
    value: BigRational,
    decimal: bool,
    rounded: bool,
    canonical: String,
}

impl ExactNumber {
    fn bounded_binary(left: &BigRational, right: &BigRational, op: char, work: &mut WorkBudget) -> Result<BigRational, DiagnosticKind> {
        work.charge(((left.numer().bits() + left.denom().bits() + right.numer().bits() + right.denom().bits()) / 32 + 1) as usize)?;
        if op == '/' && right.is_zero() { return Err(error("division by zero")); }
        if matches!(op, '*' | '/') {
            let (right_numerator, right_denominator) = if op == '*' {
                (right.numer().clone(), right.denom().clone())
            } else {
                (right.denom().clone(), right.numer().clone())
            };
            let first = left.numer().gcd(&right_denominator);
            let second = right_numerator.gcd(left.denom());
            let a = left.numer() / &first;
            let d = right_denominator / first;
            let c = right_numerator / &second;
            let b = left.denom() / second;
            if a.bits().saturating_add(c.bits()) > MAX_BITS + 1
                || b.bits().saturating_add(d.bits()) > MAX_BITS + 1
            { return Err(error("exact arithmetic intermediate exceeds the 4096-bit limit")); }
            return Ok(BigRational::new(a * c, b * d));
        }
        let gcd = left.denom().gcd(right.denom());
        let left_factor = right.denom() / &gcd;
        let right_factor = left.denom() / gcd;
        let numerator_bits = left.numer().bits().saturating_add(left_factor.bits())
            .max(right.numer().bits().saturating_add(right_factor.bits()));
        let denominator_bits = left.denom().bits().saturating_add(left_factor.bits());
        if numerator_bits > MAX_BITS || denominator_bits > MAX_BITS + 1 {
            return Err(error("exact arithmetic intermediate exceeds the 4096-bit limit"));
        }
        let first = left.numer() * &left_factor;
        let second = right.numer() * &right_factor;
        let numerator = if op == '+' { first + second } else { first - second };
        if numerator.bits() > MAX_BITS { return Err(error("exact arithmetic intermediate exceeds the 4096-bit limit")); }
        Ok(BigRational::new(numerator, left.denom() * left_factor))
    }

    pub(crate) fn combinatorial(function: EverydayFunction, args: &[CalculationValue], work: &mut WorkBudget) -> Result<CalculationValue, DiagnosticKind> {
        let [n, k] = args else { return Err(DiagnosticKind::InvalidExpression); };
        let n = Self::integer(n)?;
        let k = Self::integer(k)?;
        if n.is_negative() || k.is_negative() || k > n { return Err(error("n and k must be nonnegative integers with k no greater than n")); }
        let steps = if matches!(function, EverydayFunction::Choose) { k.clone().min(&n - &k) } else { k };
        let count = steps.to_u64().filter(|steps| *steps <= MAX_COMBINATORIAL_STEPS)
            .ok_or_else(|| error("combinatorial work exceeds the 10000-step limit"))?;
        let mut result = BigInt::one();
        for index in 0..count {
            work.charge((result.bits() / 64 + n.bits() / 64 + 1) as usize)?;
            let mut factor = &n - BigInt::from(index);
            if matches!(function, EverydayFunction::Choose) {
                let mut denominator = BigInt::from(index + 1);
                let gcd = factor.gcd(&denominator);
                factor /= &gcd;
                denominator /= gcd;
                let gcd = result.gcd(&denominator);
                result /= &gcd;
                denominator /= gcd;
                if !denominator.is_one() { return Err(error("invalid combinatorial intermediate")); }
            }
            if result.bits().saturating_add(factor.bits()) > MAX_BITS + 1 {
                return Err(error("combinatorial result exceeds the 4096-bit limit"));
            }
            result *= factor;
            if result.bits() > MAX_BITS { return Err(error("combinatorial result exceeds the 4096-bit limit")); }
        }
        if args.iter().all(|arg| matches!(arg, CalculationValue::Scalar(_)))
            && result <= BigInt::from(1u64 << 53)
        {
            return Ok(CalculationValue::Scalar(result.to_f64().ok_or_else(|| error("integer conversion failed"))?));
        }
        Ok(CalculationValue::Exact(Self::fraction(result, BigInt::one(), work)?))
    }

    pub(crate) fn everyday(function: EverydayFunction, args: &[CalculationValue], work: &mut WorkBudget) -> Result<CalculationValue, DiagnosticKind> {
        let values = args.iter().map(Self::rational).collect::<Result<Vec<_>, _>>()?;
        let decimal = values.iter().all(|(_, decimal, _)| *decimal);
        let rounded = values.iter().any(|(_, _, rounded)| *rounded);
        let values = values.into_iter().map(|(value, _, _)| value).collect::<Vec<_>>();
        let binary = |a: &BigRational, b: &BigRational, op, work: &mut WorkBudget| Self::bounded_binary(a, b, op, work);
        let result = match (function, values.as_slice()) {
            (EverydayFunction::RoundTo | EverydayFunction::FloorTo | EverydayFunction::CeilTo, [value, step]) => {
                if !step.is_positive() { return Err(error("step must be positive")); }
                let ratio = binary(value, step, '/', work)?;
                let (quotient, remainder) = ratio.numer().div_rem(ratio.denom());
                let integer = match function {
                    EverydayFunction::FloorTo => ratio.numer().div_floor(ratio.denom()),
                    EverydayFunction::CeilTo => ratio.numer().div_ceil(ratio.denom()),
                    _ if remainder.abs() * 2u8 >= *ratio.denom() => quotient + if ratio.is_negative() { -1 } else { 1 },
                    _ => quotient,
                };
                binary(&BigRational::from_integer(integer), step, '*', work)?
            }
            (EverydayFunction::Clamp, [value, min, max]) => {
                if min > max { return Err(error("clamp minimum must not exceed maximum")); }
                value.clone().max(min.clone()).min(max.clone())
            }
            (EverydayFunction::PercentChange, [new, old]) => {
                let difference = binary(new, old, '-', work)?;
                binary(&difference, old, '/', work)?
            }
            (EverydayFunction::PercentOf, [part, total]) => binary(part, total, '/', work)?,
            (EverydayFunction::Lerp, [start, end, t]) => {
                let difference = binary(end, start, '-', work)?;
                let scaled = binary(&difference, t, '*', work)?;
                binary(start, &scaled, '+', work)?
            }
            (EverydayFunction::MapRange, [value, a, b, c, d]) => {
                let offset = binary(value, a, '-', work)?;
                let width = binary(b, a, '-', work)?;
                if width.is_zero() { return Err(error("input range endpoints must differ")); }
                let fraction = binary(&offset, &width, '/', work)?;
                let output_width = binary(d, c, '-', work)?;
                let scaled = binary(&fraction, &output_width, '*', work)?;
                binary(c, &scaled, '+', work)?
            }
            _ => return Err(DiagnosticKind::InvalidExpression),
        };
        if decimal && Self::finite_scale(result.denom(), work)?.is_none() {
            return Err(error("decimal result is nonterminating; use a fraction or decimal_div"));
        }
        Ok(CalculationValue::Exact(Self::checked(result, decimal, rounded, work)?))
    }
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
        let mut result = Self { value, decimal, rounded, canonical: String::new() };
        result.canonical = result.build_canonical();
        if result.canonical.len() > MAX_EXPRESSION_BYTES {
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

    /// Formats a reduced value without converting through floating point.
    pub(crate) fn mixed_display(&self) -> String {
        let (whole, remainder) = self.value.numer().abs().div_rem(self.value.denom());
        let sign = if self.value.is_negative() { "-" } else { "" };
        let text = if remainder.is_zero() {
            format!("{sign}{whole}")
        } else if whole.is_zero() {
            format!("{sign}{remainder}/{}", self.value.denom())
        } else {
            format!("{sign}{whole} {remainder}/{}", self.value.denom())
        };
        if self.rounded { format!("≈ {text}") } else { text }
    }

    /// Decimal presentation uses at most 15 requested places when rounding is needed.
    pub(crate) fn decimal_display(&self, places: Option<u8>) -> String {
        if places.is_none() {
            if self.decimal { return self.display(); }
            let mut work = WorkBudget::default();
            if let Ok(decimal) = Self::checked(self.value.clone(), true, self.rounded, &mut work) {
                return decimal.display();
            }
        }
        let places = places.unwrap_or(12).min(15) as usize;
        let factor = BigInt::from(10u8).pow(places as u32);
        let scaled = self.value.numer() * &factor;
        let (mut coefficient, remainder) = scaled.div_rem(self.value.denom());
        let twice = remainder.abs() * 2u8;
        let ordering = twice.cmp(self.value.denom());
        if ordering == Ordering::Greater || (ordering == Ordering::Equal && coefficient.is_odd()) {
            coefficient += if scaled.sign() == Sign::Minus { -1 } else { 1 };
        }
        let negative = coefficient.is_negative();
        let mut digits = coefficient.abs().to_string();
        if places > 0 {
            if digits.len() <= places {
                digits = format!("{}{}", "0".repeat(places + 1 - digits.len()), digits);
            }
            digits.insert(digits.len() - places, '.');
            while digits.ends_with('0') { digits.pop(); }
            if digits.ends_with('.') { digits.pop(); }
        }
        let sign = if negative { "-" } else { "" };
        let marker = if self.rounded || !remainder.is_zero() { "≈ " } else { "" };
        format!("{marker}{sign}{digits}")
    }

    pub(crate) fn canonical(&self) -> String {
        self.canonical.clone()
    }

    fn build_canonical(&self) -> String {
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
