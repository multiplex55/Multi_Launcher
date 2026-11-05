//! Exact fixed-width integer values for Calculation Pad.
//!
//! Values store a width-normalized bit pattern and a signedness mode. Arithmetic
//! is checked in that mode; shifts reject counts outside the width, while left
//! shifts and bitwise operations keep only the selected width's low bits.

use crate::common::numeric_base::{NumericBase, parse_unsigned};
use std::fmt;

/// Maximum source token size accepted by [`TypedInteger::parse`].
pub const MAX_INTEGER_LITERAL_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegerWidth {
    Bits8,
    Bits16,
    Bits32,
    Bits64,
    Bits128,
}

impl IntegerWidth {
    pub const ALL: [Self; 5] = [
        Self::Bits8,
        Self::Bits16,
        Self::Bits32,
        Self::Bits64,
        Self::Bits128,
    ];

    pub const fn bits(self) -> u32 {
        match self {
            Self::Bits8 => 8,
            Self::Bits16 => 16,
            Self::Bits32 => 32,
            Self::Bits64 => 64,
            Self::Bits128 => 128,
        }
    }

    pub const fn bit_mask(self) -> u128 {
        match self {
            Self::Bits128 => u128::MAX,
            width => (1_u128 << width.bits()) - 1,
        }
    }

    const fn signed_min_magnitude(self) -> u128 {
        1_u128 << (self.bits() - 1)
    }

    const fn signed_max(self) -> u128 {
        self.signed_min_magnitude() - 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegerSignedness {
    Signed,
    Unsigned,
}

/// A width and signedness pair, such as `i32` or `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntegerMode {
    width: IntegerWidth,
    signedness: IntegerSignedness,
}

impl IntegerMode {
    pub const fn new(width: IntegerWidth, signedness: IntegerSignedness) -> Self {
        Self { width, signedness }
    }

    pub const fn signed(width: IntegerWidth) -> Self {
        Self::new(width, IntegerSignedness::Signed)
    }

    pub const fn unsigned(width: IntegerWidth) -> Self {
        Self::new(width, IntegerSignedness::Unsigned)
    }

    pub const fn width(self) -> IntegerWidth {
        self.width
    }

    pub const fn signedness(self) -> IntegerSignedness {
        self.signedness
    }

    pub const fn is_signed(self) -> bool {
        matches!(self.signedness, IntegerSignedness::Signed)
    }

    pub const fn type_name(self) -> &'static str {
        match (self.signedness, self.width) {
            (IntegerSignedness::Signed, IntegerWidth::Bits8) => "i8",
            (IntegerSignedness::Signed, IntegerWidth::Bits16) => "i16",
            (IntegerSignedness::Signed, IntegerWidth::Bits32) => "i32",
            (IntegerSignedness::Signed, IntegerWidth::Bits64) => "i64",
            (IntegerSignedness::Signed, IntegerWidth::Bits128) => "i128",
            (IntegerSignedness::Unsigned, IntegerWidth::Bits8) => "u8",
            (IntegerSignedness::Unsigned, IntegerWidth::Bits16) => "u16",
            (IntegerSignedness::Unsigned, IntegerWidth::Bits32) => "u32",
            (IntegerSignedness::Unsigned, IntegerWidth::Bits64) => "u64",
            (IntegerSignedness::Unsigned, IntegerWidth::Bits128) => "u128",
        }
    }
}

/// A fixed-width integer whose normalized bit pattern is never stored as `f64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypedInteger {
    bits: u128,
    mode: IntegerMode,
}

impl TypedInteger {
    pub const fn zero(mode: IntegerMode) -> Self {
        Self { bits: 0, mode }
    }

    pub const fn one(mode: IntegerMode) -> Self {
        Self { bits: 1, mode }
    }

    /// Construct from a nonnegative numeric value, rejecting values outside `mode`.
    pub fn from_unsigned(value: u128, mode: IntegerMode) -> Result<Self, IntegerError> {
        let maximum = if mode.is_signed() {
            mode.width.signed_max()
        } else {
            mode.width.bit_mask()
        };
        if value > maximum {
            return Err(IntegerError::OutOfRange);
        }
        Ok(Self { bits: value, mode })
    }

    /// Construct from a signed numeric value, rejecting values outside `mode`.
    pub fn from_signed(value: i128, mode: IntegerMode) -> Result<Self, IntegerError> {
        if !mode.is_signed() {
            if value < 0 {
                return Err(IntegerError::NegativeUnsigned);
            }
            return Self::from_unsigned(value as u128, mode);
        }

        let (minimum, maximum) = signed_limits(mode.width);
        if value < minimum || value > maximum {
            return Err(IntegerError::OutOfRange);
        }
        Ok(Self {
            bits: (value as u128) & mode.width.bit_mask(),
            mode,
        })
    }

    /// Construct from a normalized bit pattern without silently truncating it.
    pub fn from_bit_pattern(bits: u128, mode: IntegerMode) -> Result<Self, IntegerError> {
        if bits & !mode.width.bit_mask() != 0 {
            return Err(IntegerError::OutOfRange);
        }
        Ok(Self { bits, mode })
    }

    /// Parse a decimal, binary (`0b`), octal (`0o`), or hexadecimal (`0x`) token.
    /// A leading sign is parsed from the original text, before any floating-point path.
    pub fn parse(literal: &str, mode: IntegerMode) -> Result<Self, IntegerError> {
        if literal.len() > MAX_INTEGER_LITERAL_BYTES {
            return Err(IntegerError::LiteralTooLong {
                max_bytes: MAX_INTEGER_LITERAL_BYTES,
            });
        }
        if literal.is_empty() {
            return Err(IntegerError::InvalidLiteral);
        }

        let (negative, unsigned_literal) = match literal.as_bytes()[0] {
            b'-' => (true, &literal[1..]),
            b'+' => (false, &literal[1..]),
            _ => (false, literal),
        };
        if unsigned_literal.is_empty() {
            return Err(IntegerError::InvalidLiteral);
        }

        let (base, digits) = if let Some(rest) = unsigned_literal
            .strip_prefix("0x")
            .or_else(|| unsigned_literal.strip_prefix("0X"))
        {
            (NumericBase::Hexadecimal, rest)
        } else if let Some(rest) = unsigned_literal
            .strip_prefix("0o")
            .or_else(|| unsigned_literal.strip_prefix("0O"))
        {
            (NumericBase::Octal, rest)
        } else if let Some(rest) = unsigned_literal
            .strip_prefix("0b")
            .or_else(|| unsigned_literal.strip_prefix("0B"))
        {
            (NumericBase::Binary, rest)
        } else {
            (NumericBase::Decimal, unsigned_literal)
        };
        if digits.is_empty() {
            return Err(IntegerError::InvalidLiteral);
        }
        if !digits.bytes().all(|digit| valid_digit(digit, base)) {
            return Err(IntegerError::InvalidLiteral);
        }
        let magnitude = parse_unsigned(digits, base).ok_or(IntegerError::LiteralOutOfRange)?;

        match mode.signedness {
            IntegerSignedness::Unsigned => {
                if negative {
                    return Err(IntegerError::NegativeUnsigned);
                }
                Self::from_unsigned(magnitude, mode)
            }
            IntegerSignedness::Signed => {
                let minimum_magnitude = mode.width.signed_min_magnitude();
                if negative {
                    if magnitude > minimum_magnitude {
                        return Err(IntegerError::OutOfRange);
                    }
                    Self::from_bit_pattern(magnitude.wrapping_neg() & mode.width.bit_mask(), mode)
                } else {
                    Self::from_unsigned(magnitude, mode)
                }
            }
        }
    }

    pub const fn mode(self) -> IntegerMode {
        self.mode
    }

    /// The low `width` bits, normalized so all higher bits are zero.
    pub const fn bit_pattern(self) -> u128 {
        self.bits
    }

    /// Exact decimal display using this value's signedness interpretation.
    pub fn formatted_decimal(self) -> String {
        match self.mode.signedness {
            IntegerSignedness::Signed => self.signed_value().to_string(),
            IntegerSignedness::Unsigned => self.bits.to_string(),
        }
    }

    /// Explicit numeric cast. The numeric value must fit in the target mode.
    pub fn checked_cast(self, target: IntegerMode) -> Result<Self, IntegerError> {
        if self.mode.is_signed() {
            Self::from_signed(self.signed_value(), target)
        } else {
            Self::from_unsigned(self.bits, target)
        }
    }

    /// Explicit bit reinterpretation. Widening zero-extends the stored pattern;
    /// narrowing keeps the target width's low bits. The target sign bit then
    /// determines how the result is interpreted as a number.
    pub const fn reinterpret_bits(self, target: IntegerMode) -> Self {
        Self {
            bits: self.bits & target.width.bit_mask(),
            mode: target,
        }
    }

    pub fn checked_add(self, other: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(other)?;
        if self.mode.is_signed() {
            let result = self
                .signed_value()
                .checked_add(other.signed_value())
                .ok_or(IntegerError::Overflow)?;
            Self::from_signed(result, self.mode).map_err(|_| IntegerError::Overflow)
        } else {
            let result = self
                .bits
                .checked_add(other.bits)
                .ok_or(IntegerError::Overflow)?;
            Self::from_unsigned(result, self.mode).map_err(|_| IntegerError::Overflow)
        }
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(other)?;
        if self.mode.is_signed() {
            let result = self
                .signed_value()
                .checked_sub(other.signed_value())
                .ok_or(IntegerError::Overflow)?;
            Self::from_signed(result, self.mode).map_err(|_| IntegerError::Overflow)
        } else {
            let result = self
                .bits
                .checked_sub(other.bits)
                .ok_or(IntegerError::Overflow)?;
            Self::from_unsigned(result, self.mode).map_err(|_| IntegerError::Overflow)
        }
    }

    pub fn checked_mul(self, other: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(other)?;
        if self.mode.is_signed() {
            let result = self
                .signed_value()
                .checked_mul(other.signed_value())
                .ok_or(IntegerError::Overflow)?;
            Self::from_signed(result, self.mode).map_err(|_| IntegerError::Overflow)
        } else {
            let result = self
                .bits
                .checked_mul(other.bits)
                .ok_or(IntegerError::Overflow)?;
            Self::from_unsigned(result, self.mode).map_err(|_| IntegerError::Overflow)
        }
    }

    pub fn checked_div(self, other: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(other)?;
        if other.bits == 0 {
            return Err(IntegerError::DivisionByZero);
        }
        if self.mode.is_signed() {
            let result = self
                .signed_value()
                .checked_div(other.signed_value())
                .ok_or(IntegerError::Overflow)?;
            Self::from_signed(result, self.mode).map_err(|_| IntegerError::Overflow)
        } else {
            Ok(Self {
                bits: self.bits / other.bits,
                mode: self.mode,
            })
        }
    }

    pub fn checked_rem(self, other: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(other)?;
        if other.bits == 0 {
            return Err(IntegerError::DivisionByZero);
        }
        if self.mode.is_signed() {
            if self.signed_value() == signed_limits(self.mode.width).0 && other.signed_value() == -1
            {
                return Err(IntegerError::Overflow);
            }
            let result = self
                .signed_value()
                .checked_rem(other.signed_value())
                .ok_or(IntegerError::Overflow)?;
            Self::from_signed(result, self.mode).map_err(|_| IntegerError::Overflow)
        } else {
            Ok(Self {
                bits: self.bits % other.bits,
                mode: self.mode,
            })
        }
    }

    /// Bounded logarithmic exponentiation by squaring. The exponent must use
    /// the same mode and be nonnegative when signed; at most 128 iterations run.
    pub fn checked_pow(self, exponent: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(exponent)?;
        let mut exponent = if exponent.mode.is_signed() {
            let signed = exponent.signed_value();
            if signed < 0 {
                return Err(IntegerError::NegativeExponent);
            }
            signed as u128
        } else {
            exponent.bits
        };

        let mut result = Self::one(self.mode);
        let mut base = self;
        while exponent != 0 {
            if exponent & 1 != 0 {
                result = result.checked_mul(base)?;
            }
            exponent >>= 1;
            if exponent != 0 {
                base = base.checked_mul(base)?;
            }
        }
        Ok(result)
    }

    pub fn bit_and(self, other: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(other)?;
        Ok(Self {
            bits: self.bits & other.bits,
            mode: self.mode,
        })
    }

    pub fn bit_or(self, other: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(other)?;
        Ok(Self {
            bits: self.bits | other.bits,
            mode: self.mode,
        })
    }

    pub fn bit_xor(self, other: Self) -> Result<Self, IntegerError> {
        self.ensure_same_mode(other)?;
        Ok(Self {
            bits: self.bits ^ other.bits,
            mode: self.mode,
        })
    }

    pub const fn bit_not(self) -> Self {
        Self {
            bits: !self.bits & self.mode.width.bit_mask(),
            mode: self.mode,
        }
    }

    /// Shift counts must satisfy `0 <= count < width`. Left shifts discard
    /// shifted-out bits and keep the result normalized to the source width.
    pub fn shift_left(self, count: i128) -> Result<Self, IntegerError> {
        let count = self.shift_count(count)?;
        Ok(Self {
            bits: (self.bits << count) & self.mode.width.bit_mask(),
            mode: self.mode,
        })
    }

    /// Signed right shifts extend the sign bit; unsigned right shifts fill with zero.
    pub fn shift_right(self, count: i128) -> Result<Self, IntegerError> {
        let count = self.shift_count(count)?;
        if self.mode.is_signed() {
            Self::from_signed(self.signed_value() >> count, self.mode)
        } else {
            Ok(Self {
                bits: self.bits >> count,
                mode: self.mode,
            })
        }
    }

    /// Rotate by a nonnegative count reduced modulo the selected width.
    pub fn rotate_left(self, count: i128) -> Result<Self, IntegerError> {
        self.rotate(count, true)
    }

    /// Rotate by a nonnegative count reduced modulo the selected width.
    pub fn rotate_right(self, count: i128) -> Result<Self, IntegerError> {
        self.rotate(count, false)
    }

    pub const fn popcount(self) -> u32 {
        self.bits.count_ones()
    }

    /// Return a same-mode mask with `count` consecutive low-order field bits set
    /// at `start`. Empty fields are allowed, including at `start == width`.
    pub fn field_mask(self, start: u32, count: u32) -> Result<Self, IntegerError> {
        let mask = checked_field_mask(self.mode.width, start, count)?;
        Ok(Self {
            bits: mask,
            mode: self.mode,
        })
    }

    /// Extract an LSB-indexed field as an unsigned value of the same width.
    pub fn extract_bits(self, start: u32, count: u32) -> Result<Self, IntegerError> {
        checked_field_mask(self.mode.width, start, count)?;
        let bits = if count == 0 {
            0
        } else {
            (self.bits >> start) & low_mask(count)
        };
        Self::from_bit_pattern(bits, IntegerMode::unsigned(self.mode.width))
    }

    fn ensure_same_mode(self, other: Self) -> Result<(), IntegerError> {
        if self.mode == other.mode {
            Ok(())
        } else {
            Err(IntegerError::ModeMismatch)
        }
    }

    fn signed_value(self) -> i128 {
        let width = self.mode.width;
        let sign_bit = 1_u128 << (width.bits() - 1);
        if self.bits & sign_bit == 0 {
            self.bits as i128
        } else if width == IntegerWidth::Bits128 {
            self.bits as i128
        } else {
            (self.bits | !width.bit_mask()) as i128
        }
    }

    fn shift_count(self, count: i128) -> Result<u32, IntegerError> {
        if count < 0 || count >= i128::from(self.mode.width.bits()) {
            return Err(IntegerError::InvalidShiftCount {
                count,
                width: self.mode.width,
            });
        }
        Ok(count as u32)
    }

    fn rotate(self, count: i128, left: bool) -> Result<Self, IntegerError> {
        if count < 0 {
            return Err(IntegerError::InvalidShiftCount {
                count,
                width: self.mode.width,
            });
        }
        let width = self.mode.width.bits();
        let count = (count as u128 % u128::from(width)) as u32;
        let bits = if count == 0 {
            self.bits
        } else if left {
            ((self.bits << count) | (self.bits >> (width - count))) & self.mode.width.bit_mask()
        } else {
            ((self.bits >> count) | (self.bits << (width - count))) & self.mode.width.bit_mask()
        };
        Ok(Self {
            bits,
            mode: self.mode,
        })
    }
}

fn signed_limits(width: IntegerWidth) -> (i128, i128) {
    if width == IntegerWidth::Bits128 {
        (i128::MIN, i128::MAX)
    } else {
        let magnitude = 1_i128 << (width.bits() - 1);
        (-magnitude, magnitude - 1)
    }
}

fn valid_digit(digit: u8, base: NumericBase) -> bool {
    match base {
        NumericBase::Binary => matches!(digit, b'0' | b'1'),
        NumericBase::Octal => (b'0'..=b'7').contains(&digit),
        NumericBase::Decimal => digit.is_ascii_digit(),
        NumericBase::Hexadecimal => digit.is_ascii_hexdigit(),
    }
}

fn low_mask(count: u32) -> u128 {
    match count {
        0 => 0,
        128 => u128::MAX,
        count => (1_u128 << count) - 1,
    }
}

fn checked_field_mask(width: IntegerWidth, start: u32, count: u32) -> Result<u128, IntegerError> {
    let Some(end) = start.checked_add(count) else {
        return Err(IntegerError::InvalidBitRange {
            start,
            count,
            width,
        });
    };
    if start > width.bits() || end > width.bits() {
        return Err(IntegerError::InvalidBitRange {
            start,
            count,
            width,
        });
    }
    if count == 0 {
        return Ok(0);
    }
    Ok((low_mask(count) << start) & width.bit_mask())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerError {
    InvalidLiteral,
    LiteralTooLong {
        max_bytes: usize,
    },
    LiteralOutOfRange,
    NegativeUnsigned,
    OutOfRange,
    ModeMismatch,
    Overflow,
    DivisionByZero,
    NegativeExponent,
    InvalidShiftCount {
        count: i128,
        width: IntegerWidth,
    },
    InvalidBitRange {
        start: u32,
        count: u32,
        width: IntegerWidth,
    },
}

impl fmt::Display for IntegerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLiteral => formatter.write_str("invalid integer literal"),
            Self::LiteralTooLong { max_bytes } => {
                write!(
                    formatter,
                    "integer literal exceeds the {max_bytes}-byte limit"
                )
            }
            Self::LiteralOutOfRange | Self::OutOfRange => {
                formatter.write_str("integer value is outside the selected range")
            }
            Self::NegativeUnsigned => {
                formatter.write_str("negative values cannot be represented as unsigned integers")
            }
            Self::ModeMismatch => {
                formatter.write_str("integer operands must use the same width and signedness")
            }
            Self::Overflow => formatter.write_str("integer arithmetic overflowed"),
            Self::DivisionByZero => formatter.write_str("integer division by zero"),
            Self::NegativeExponent => formatter.write_str("integer exponent cannot be negative"),
            Self::InvalidShiftCount { count, width } => write!(
                formatter,
                "shift or rotate count {count} is outside the 0..{} range",
                width.bits()
            ),
            Self::InvalidBitRange {
                start,
                count,
                width,
            } => write!(
                formatter,
                "bit range starting at {start} with length {count} exceeds {} bits",
                width.bits()
            ),
        }
    }
}

impl std::error::Error for IntegerError {}

#[cfg(test)]
mod tests {
    use super::{
        IntegerError, IntegerMode, IntegerSignedness, IntegerWidth, MAX_INTEGER_LITERAL_BYTES,
        TypedInteger,
    };
    use crate::calculation_pad::CalculationValue;

    fn s(width: IntegerWidth, value: i128) -> TypedInteger {
        TypedInteger::from_signed(value, IntegerMode::signed(width)).unwrap()
    }

    fn u(width: IntegerWidth, value: u128) -> TypedInteger {
        TypedInteger::from_unsigned(value, IntegerMode::unsigned(width)).unwrap()
    }

    fn bit_pattern(value: TypedInteger) -> u128 {
        value.bit_pattern()
    }

    #[test]
    fn signed_and_unsigned_widths_accept_exact_endpoints() {
        for width in IntegerWidth::ALL {
            let mask = width.bit_mask();
            let unsigned = IntegerMode::unsigned(width);
            assert_eq!(bit_pattern(TypedInteger::zero(unsigned)), 0);
            assert_eq!(
                bit_pattern(TypedInteger::from_unsigned(mask, unsigned).unwrap()),
                mask
            );
            if width != IntegerWidth::Bits128 {
                assert_eq!(
                    TypedInteger::from_unsigned(mask + 1, unsigned),
                    Err(IntegerError::OutOfRange)
                );
            }

            let signed = IntegerMode::signed(width);
            if width == IntegerWidth::Bits128 {
                assert_eq!(
                    TypedInteger::from_signed(i128::MIN, signed)
                        .unwrap()
                        .formatted_decimal(),
                    i128::MIN.to_string()
                );
                assert_eq!(
                    TypedInteger::from_signed(i128::MAX, signed)
                        .unwrap()
                        .formatted_decimal(),
                    i128::MAX.to_string()
                );
            } else {
                let magnitude = 1_i128 << (width.bits() - 1);
                let minimum = -magnitude;
                let maximum = magnitude - 1;
                let minimum_value = TypedInteger::from_signed(minimum, signed).unwrap();
                let maximum_value = TypedInteger::from_signed(maximum, signed).unwrap();
                assert_eq!(minimum_value.formatted_decimal(), minimum.to_string());
                assert_eq!(maximum_value.formatted_decimal(), maximum.to_string());
                assert_eq!(
                    TypedInteger::from_signed(minimum - 1, signed),
                    Err(IntegerError::OutOfRange)
                );
                assert_eq!(
                    TypedInteger::from_signed(maximum + 1, signed),
                    Err(IntegerError::OutOfRange)
                );
            }
        }
    }

    #[test]
    fn parses_signed_decimal_and_prefixed_literals_without_float_rounding() {
        let u64_mode = IntegerMode::unsigned(IntegerWidth::Bits64);
        assert_eq!(
            TypedInteger::parse("9007199254740993", u64_mode)
                .unwrap()
                .formatted_decimal(),
            "9007199254740993"
        );
        assert_eq!(
            TypedInteger::parse("0x20000000000001", u64_mode)
                .unwrap()
                .bit_pattern(),
            9_007_199_254_740_993
        );
        assert_eq!(
            TypedInteger::parse("0b101010", u64_mode)
                .unwrap()
                .formatted_decimal(),
            "42"
        );
        assert_eq!(
            TypedInteger::parse("0o52", u64_mode)
                .unwrap()
                .formatted_decimal(),
            "42"
        );

        let i8_mode = IntegerMode::signed(IntegerWidth::Bits8);
        assert_eq!(
            TypedInteger::parse("-0x80", i8_mode)
                .unwrap()
                .formatted_decimal(),
            "-128"
        );
        assert_eq!(
            TypedInteger::parse("+127", i8_mode)
                .unwrap()
                .formatted_decimal(),
            "127"
        );
        assert_eq!(
            TypedInteger::parse("-0", i8_mode)
                .unwrap()
                .formatted_decimal(),
            "0"
        );
        assert_eq!(
            TypedInteger::parse(
                "-170141183460469231731687303715884105728",
                IntegerMode::signed(IntegerWidth::Bits128)
            )
            .unwrap()
            .formatted_decimal(),
            i128::MIN.to_string()
        );
        assert_eq!(
            TypedInteger::parse(
                "340282366920938463463374607431768211455",
                IntegerMode::unsigned(IntegerWidth::Bits128)
            )
            .unwrap()
            .bit_pattern(),
            u128::MAX
        );
    }

    #[test]
    fn literal_errors_distinguish_syntax_length_and_range() {
        let u8_mode = IntegerMode::unsigned(IntegerWidth::Bits8);
        assert_eq!(
            TypedInteger::parse("0b102", u8_mode),
            Err(IntegerError::InvalidLiteral)
        );
        assert_eq!(
            TypedInteger::parse("-1", u8_mode),
            Err(IntegerError::NegativeUnsigned)
        );
        assert_eq!(
            TypedInteger::parse("256", u8_mode),
            Err(IntegerError::OutOfRange)
        );
        assert_eq!(
            TypedInteger::parse(
                "340282366920938463463374607431768211456",
                IntegerMode::unsigned(IntegerWidth::Bits128)
            ),
            Err(IntegerError::LiteralOutOfRange)
        );
        assert_eq!(
            TypedInteger::parse(&"1".repeat(MAX_INTEGER_LITERAL_BYTES + 1), u8_mode),
            Err(IntegerError::LiteralTooLong {
                max_bytes: MAX_INTEGER_LITERAL_BYTES
            })
        );
    }

    #[test]
    fn checked_arithmetic_rejects_overflow_zero_and_mode_mismatch() {
        let i8_mode = IntegerMode::signed(IntegerWidth::Bits8);
        let u8_mode = IntegerMode::unsigned(IntegerWidth::Bits8);
        assert_eq!(
            s(IntegerWidth::Bits8, 127).checked_add(s(IntegerWidth::Bits8, 1)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            s(IntegerWidth::Bits8, -128).checked_sub(s(IntegerWidth::Bits8, 1)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            s(IntegerWidth::Bits8, -65).checked_mul(s(IntegerWidth::Bits8, 2)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            s(IntegerWidth::Bits8, -128).checked_div(s(IntegerWidth::Bits8, -1)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            s(IntegerWidth::Bits8, -128).checked_rem(s(IntegerWidth::Bits8, -1)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            u(IntegerWidth::Bits8, 255).checked_add(u(IntegerWidth::Bits8, 1)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            u(IntegerWidth::Bits8, 0).checked_sub(u(IntegerWidth::Bits8, 1)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            u(IntegerWidth::Bits8, 3).checked_div(u(IntegerWidth::Bits8, 0)),
            Err(IntegerError::DivisionByZero)
        );
        assert_eq!(
            u(IntegerWidth::Bits8, 3).checked_rem(u(IntegerWidth::Bits8, 0)),
            Err(IntegerError::DivisionByZero)
        );
        assert_eq!(
            u(IntegerWidth::Bits8, 1).checked_add(
                TypedInteger::from_unsigned(1, IntegerMode::unsigned(IntegerWidth::Bits16))
                    .unwrap()
            ),
            Err(IntegerError::ModeMismatch)
        );
        assert_eq!(
            u(IntegerWidth::Bits128, u128::MAX).checked_add(u(IntegerWidth::Bits128, 1)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            s(IntegerWidth::Bits128, i128::MIN).checked_div(s(IntegerWidth::Bits128, -1)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(i8_mode.signedness(), IntegerSignedness::Signed);
        assert_eq!(u8_mode.type_name(), "u8");
    }

    #[test]
    fn exponentiation_by_squaring_is_checked_and_bounded() {
        let mode = IntegerMode::signed(IntegerWidth::Bits8);
        assert_eq!(
            s(IntegerWidth::Bits8, -2)
                .checked_pow(s(IntegerWidth::Bits8, 6))
                .unwrap()
                .formatted_decimal(),
            "64"
        );
        assert_eq!(
            s(IntegerWidth::Bits8, 2).checked_pow(s(IntegerWidth::Bits8, 7)),
            Err(IntegerError::Overflow)
        );
        assert_eq!(
            s(IntegerWidth::Bits8, 2).checked_pow(s(IntegerWidth::Bits8, -1)),
            Err(IntegerError::NegativeExponent)
        );
        assert_eq!(
            s(IntegerWidth::Bits8, 7)
                .checked_pow(s(IntegerWidth::Bits8, 0))
                .unwrap(),
            TypedInteger::one(mode)
        );
        assert_eq!(
            u(IntegerWidth::Bits8, 3)
                .checked_pow(u(IntegerWidth::Bits8, 5))
                .unwrap()
                .formatted_decimal(),
            "243"
        );
        let u128_mode = IntegerMode::unsigned(IntegerWidth::Bits128);
        assert_eq!(
            TypedInteger::one(u128_mode)
                .checked_pow(TypedInteger::from_unsigned(u128::MAX, u128_mode).unwrap())
                .unwrap(),
            TypedInteger::one(u128_mode)
        );
    }

    #[test]
    fn bitwise_operations_are_width_masked() {
        let mode = IntegerMode::unsigned(IntegerWidth::Bits8);
        let left = u(IntegerWidth::Bits8, 0xa5);
        let right = u(IntegerWidth::Bits8, 0x3c);
        assert_eq!(bit_pattern(left.bit_and(right).unwrap()), 0x24);
        assert_eq!(bit_pattern(left.bit_or(right).unwrap()), 0xbd);
        assert_eq!(bit_pattern(left.bit_xor(right).unwrap()), 0x99);
        assert_eq!(bit_pattern(TypedInteger::zero(mode).bit_not()), 0xff);
        assert_eq!(
            bit_pattern(
                TypedInteger::from_bit_pattern(0xff, mode)
                    .unwrap()
                    .bit_not()
            ),
            0
        );
        assert_eq!(
            TypedInteger::from_bit_pattern(0xff, mode)
                .unwrap()
                .bit_and(u(IntegerWidth::Bits8, 0xff))
                .unwrap()
                .mode(),
            mode
        );
    }

    #[test]
    fn shifts_validate_counts_mask_left_bits_and_extend_signed_right_shifts() {
        let u8_min = u(IntegerWidth::Bits8, 0x81);
        assert_eq!(bit_pattern(u8_min.shift_left(1).unwrap()), 0x02);
        assert_eq!(bit_pattern(u8_min.shift_right(1).unwrap()), 0x40);
        assert_eq!(
            s(IntegerWidth::Bits8, -128)
                .shift_right(1)
                .unwrap()
                .formatted_decimal(),
            "-64"
        );
        assert_eq!(
            bit_pattern(s(IntegerWidth::Bits8, -1).shift_left(0).unwrap()),
            0xff
        );
        assert_eq!(
            u8_min.shift_left(-1),
            Err(IntegerError::InvalidShiftCount {
                count: -1,
                width: IntegerWidth::Bits8
            })
        );
        assert_eq!(
            u8_min.shift_right(8),
            Err(IntegerError::InvalidShiftCount {
                count: 8,
                width: IntegerWidth::Bits8
            })
        );

        for width in IntegerWidth::ALL {
            let mode = IntegerMode::unsigned(width);
            let value = TypedInteger::from_bit_pattern(width.bit_mask(), mode).unwrap();
            assert_eq!(
                value
                    .shift_left(i128::from(width.bits() - 1))
                    .unwrap()
                    .bit_pattern(),
                1_u128 << (width.bits() - 1)
            );
            assert_eq!(
                value
                    .shift_right(i128::from(width.bits() - 1))
                    .unwrap()
                    .bit_pattern(),
                1
            );
        }
    }

    #[test]
    fn rotations_reduce_nonnegative_counts_modulo_width() {
        let value = u(IntegerWidth::Bits8, 0x81);
        assert_eq!(bit_pattern(value.rotate_left(0).unwrap()), 0x81);
        assert_eq!(bit_pattern(value.rotate_left(1).unwrap()), 0x03);
        assert_eq!(bit_pattern(value.rotate_right(1).unwrap()), 0xc0);
        assert_eq!(bit_pattern(value.rotate_left(8).unwrap()), 0x81);
        assert_eq!(bit_pattern(value.rotate_left(1009).unwrap()), 0x03);
        assert_eq!(
            value.rotate_right(-1),
            Err(IntegerError::InvalidShiftCount {
                count: -1,
                width: IntegerWidth::Bits8
            })
        );
    }

    #[test]
    fn popcount_masks_and_extraction_check_empty_full_and_overflowing_ranges() {
        let value = u(IntegerWidth::Bits8, 0xb5);
        assert_eq!(value.popcount(), 5);
        assert_eq!(bit_pattern(value.field_mask(0, 8).unwrap()), 0xff);
        assert_eq!(bit_pattern(value.field_mask(8, 0).unwrap()), 0);
        assert_eq!(bit_pattern(value.extract_bits(2, 4).unwrap()), 0x0d);
        assert_eq!(bit_pattern(value.extract_bits(0, 8).unwrap()), 0xb5);
        assert_eq!(
            value.extract_bits(7, 2),
            Err(IntegerError::InvalidBitRange {
                start: 7,
                count: 2,
                width: IntegerWidth::Bits8,
            })
        );
        assert!(matches!(
            value.field_mask(u32::MAX, 1),
            Err(IntegerError::InvalidBitRange { .. })
        ));
        let wide = u(IntegerWidth::Bits128, u128::MAX);
        assert_eq!(wide.field_mask(0, 128).unwrap().bit_pattern(), u128::MAX);
        assert_eq!(wide.extract_bits(0, 128).unwrap().bit_pattern(), u128::MAX);
    }

    #[test]
    fn checked_casts_and_bit_reinterpretation_are_distinct() {
        let signed_minus_one = s(IntegerWidth::Bits8, -1);
        let u16_mode = IntegerMode::unsigned(IntegerWidth::Bits16);
        assert_eq!(
            signed_minus_one.checked_cast(u16_mode),
            Err(IntegerError::NegativeUnsigned)
        );
        let widened_pattern = signed_minus_one.reinterpret_bits(u16_mode);
        assert_eq!(bit_pattern(widened_pattern), 255);
        assert_eq!(widened_pattern.formatted_decimal(), "255");
        assert_eq!(
            u(IntegerWidth::Bits16, 255)
                .reinterpret_bits(IntegerMode::signed(IntegerWidth::Bits8))
                .formatted_decimal(),
            "-1"
        );
        assert_eq!(
            s(IntegerWidth::Bits8, -1)
                .checked_cast(IntegerMode::signed(IntegerWidth::Bits16))
                .unwrap()
                .formatted_decimal(),
            "-1"
        );
        assert_eq!(
            u(IntegerWidth::Bits8, 255).checked_cast(IntegerMode::signed(IntegerWidth::Bits8)),
            Err(IntegerError::OutOfRange)
        );
    }

    #[test]
    fn calculation_integer_results_format_exact_decimal_without_changing_base_results() {
        let integer = TypedInteger::parse(
            "9007199254740993",
            IntegerMode::unsigned(IntegerWidth::Bits64),
        )
        .unwrap();
        assert_eq!(
            CalculationValue::Integer(integer)
                .formatted_result()
                .as_deref(),
            Some("9007199254740993")
        );
        assert_eq!(
            CalculationValue::ExactBase {
                value: 9_007_199_254_740_993,
                base: crate::common::numeric_base::NumericBase::Hexadecimal,
                digits: "20000000000001".into(),
            }
            .formatted_result()
            .as_deref(),
            Some("20000000000001")
        );
        let signed = TypedInteger::parse("-128", IntegerMode::signed(IntegerWidth::Bits8)).unwrap();
        assert_eq!(signed.mode().type_name(), "i8");
        assert_eq!(
            CalculationValue::Integer(signed)
                .formatted_result()
                .as_deref(),
            Some("-128")
        );
    }
}
