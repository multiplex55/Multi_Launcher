//! Source-backed numeric input configuration and pure draft projection.
use std::cmp::Ordering;

use num_bigint::{BigInt, Sign};
use serde::{Deserialize, Serialize};

use super::document::script::WorkBudget;
use super::document::{CalculationDocument, CalculationValue, LineOutcome, LiteralInput};
use super::exact::ExactNumber;
use super::integer::{IntegerMode, IntegerWidth, TypedInteger};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InputKind {
    Scalar,
    ExactInteger,
    Integer(IntegerMode),
    Decimal,
    Quantity { unit: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InputControl {
    #[default]
    Field,
    Stepper,
    Slider,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum InputAnchor {
    Scalar,
    ExactInteger,
    Integer { mode: String },
    Decimal,
    Quantity { unit: String },
}

impl InputKind {
    pub(crate) fn anchor(&self) -> InputAnchor {
        match self {
            Self::Scalar => InputAnchor::Scalar,
            Self::ExactInteger => InputAnchor::ExactInteger,
            Self::Integer(mode) => InputAnchor::Integer {
                mode: mode.type_name().into(),
            },
            Self::Decimal => InputAnchor::Decimal,
            Self::Quantity { unit } => InputAnchor::Quantity { unit: unit.clone() },
        }
    }
}

impl InputAnchor {
    fn kind(&self) -> Result<InputKind, &'static str> {
        Ok(match self {
            Self::Scalar => InputKind::Scalar,
            Self::ExactInteger => InputKind::ExactInteger,
            Self::Decimal => InputKind::Decimal,
            Self::Quantity { unit } => {
                if unit.is_empty()
                    || unit.len() > 128
                    || unit.chars().any(char::is_control)
                    || (crate::unit_conversion::unit_by_alias(unit).is_none()
                        && super::quantity::parse_unit_target(unit).is_err())
                {
                    return Err("Invalid input unit anchor.");
                }
                InputKind::Quantity { unit: unit.clone() }
            }
            Self::Integer { mode } => {
                let (signed, width) = match mode.as_str() {
                    "i8" => (true, IntegerWidth::Bits8),
                    "i16" => (true, IntegerWidth::Bits16),
                    "i32" => (true, IntegerWidth::Bits32),
                    "i64" => (true, IntegerWidth::Bits64),
                    "i128" => (true, IntegerWidth::Bits128),
                    "u8" => (false, IntegerWidth::Bits8),
                    "u16" => (false, IntegerWidth::Bits16),
                    "u32" => (false, IntegerWidth::Bits32),
                    "u64" => (false, IntegerWidth::Bits64),
                    "u128" => (false, IntegerWidth::Bits128),
                    _ => return Err("Invalid input integer type."),
                };
                InputKind::Integer(if signed {
                    IntegerMode::signed(width)
                } else {
                    IntegerMode::unsigned(width)
                })
            }
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct InputConfig {
    pub label: String,
    pub description: String,
    pub suffix: String,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub step: Option<String>,
    pub control: InputControl,
    pub anchor: Option<InputAnchor>,
}

impl InputConfig {
    pub(crate) fn has_numeric_constraints(&self) -> bool {
        self.minimum.is_some() || self.maximum.is_some() || self.step.is_some()
    }

    fn validate_shape(&self) -> Result<(), &'static str> {
        if self.label.len() > 256
            || self.description.len() > 1024
            || self.suffix.len() > 128
            || [&self.label, &self.description, &self.suffix]
                .iter()
                .any(|value| value.chars().any(char::is_control))
        {
            return Err(
                "Input labels or descriptions exceed their limits or contain control characters.",
            );
        }
        if [&self.minimum, &self.maximum, &self.step]
            .into_iter()
            .flatten()
            .any(|value| {
                value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
            })
        {
            return Err("Input bounds or step are empty, too long, or contain control characters.");
        }
        if self.anchor.is_none() && self.has_numeric_constraints() {
            return Err("Input bounds require a source type anchor.");
        }
        Ok(())
    }

    pub(crate) fn validate_storage(&self) -> Result<(), &'static str> {
        self.validate_shape()?;
        if let Some(anchor) = &self.anchor {
            let kind = anchor.kind()?;
            validate_numbers(&kind, self).map_err(|_| "Invalid input bounds or step.")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum Number {
    Float(f64),
    Integer(TypedInteger),
    ExactInteger(BigInt),
    Decimal(ExactNumber),
}

fn parse(kind: &InputKind, source: &str) -> Result<Number, String> {
    if source.is_empty()
        || source.len() > super::document::MAX_EXPRESSION_BYTES
        || source.trim() != source
        || source.contains(['\r', '\n'])
    {
        return Err("Enter one bounded numeric literal.".into());
    }
    match kind {
        InputKind::Integer(mode) => TypedInteger::parse(source, *mode)
            .map(Number::Integer)
            .map_err(|error| error.to_string()),
        InputKind::ExactInteger => {
            if !super::document::integer_literal_token(source) {
                return Err("Enter an integer literal.".into());
            }
            let negative = source.starts_with('-');
            let unsigned = source.trim_start_matches(['+', '-']);
            let (digits, radix) = if let Some(value) = unsigned
                .strip_prefix("0x")
                .or_else(|| unsigned.strip_prefix("0X"))
            {
                (value, 16)
            } else if let Some(value) = unsigned
                .strip_prefix("0b")
                .or_else(|| unsigned.strip_prefix("0B"))
            {
                (value, 2)
            } else if let Some(value) = unsigned
                .strip_prefix("0o")
                .or_else(|| unsigned.strip_prefix("0O"))
            {
                (value, 8)
            } else {
                (unsigned, 10)
            };
            let digits = digits.replace('_', "");
            let mut value =
                BigInt::parse_bytes(digits.as_bytes(), radix).ok_or("Invalid integer literal.")?;
            if value.bits() > 4096 {
                return Err("Integer exceeds the exact limit.".into());
            }
            if negative && value.sign() != Sign::NoSign {
                value = -value;
            }
            Ok(Number::ExactInteger(value))
        }
        InputKind::Decimal => ExactNumber::decimal(source, &mut WorkBudget::default())
            .map(Number::Decimal)
            .map_err(|kind| super::document::Diagnostic { kind }.to_string()),
        InputKind::Scalar | InputKind::Quantity { .. } => {
            let document = CalculationDocument::new(source);
            let evaluation = document.evaluate();
            match evaluation.lines.first().map(|line| &line.outcome) {
                Some(LineOutcome::Value(CalculationValue::Scalar(value)))
                    if value.is_finite()
                        && (crate::unit_conversion::parse_quantity_number_prefix(source)
                            .is_ok_and(|(_, end)| end == source.len())
                            || super::document::integer_literal_token(source)) =>
                {
                    Ok(Number::Float(*value))
                }
                _ => Err("Enter a finite numeric literal.".into()),
            }
        }
    }
}

fn compare(left: &Number, right: &Number) -> Result<Ordering, String> {
    match (left, right) {
        (Number::Float(a), Number::Float(b)) => a.partial_cmp(b).ok_or("Invalid number".into()),
        (Number::Integer(a), Number::Integer(b)) if a.mode() == b.mode() => {
            if a.mode().is_signed() {
                Ok(a.as_i128().cmp(&b.as_i128()))
            } else {
                Ok(a.bit_pattern().cmp(&b.bit_pattern()))
            }
        }
        (Number::ExactInteger(a), Number::ExactInteger(b)) => Ok(a.cmp(b)),
        (Number::Decimal(a), Number::Decimal(b)) => ExactNumber::compare(
            &CalculationValue::Exact(a.clone()),
            &CalculationValue::Exact(b.clone()),
            &mut WorkBudget::default(),
        )
        .map_err(|kind| super::document::Diagnostic { kind }.to_string()),
        _ => Err("Input values have incompatible types.".into()),
    }
}

fn zero(kind: &InputKind) -> Result<Number, String> {
    parse(kind, "0")
}

#[derive(Clone, Debug)]
pub(crate) struct InputProjection {
    pub descriptor: LiteralInput,
    pub config: InputConfig,
    pub current_error: Option<String>,
    pub slider_available: bool,
}

pub(crate) fn validate_config(kind: &InputKind, config: &InputConfig) -> Result<(), String> {
    config.validate_shape().map_err(str::to_owned)?;
    if config.has_numeric_constraints()
        && config
            .anchor
            .as_ref()
            .is_some_and(|anchor| *anchor != kind.anchor())
    {
        return Err("Input type or unit changed; review its settings.".into());
    }
    validate_numbers(kind, config)
}

fn validate_numbers(kind: &InputKind, config: &InputConfig) -> Result<(), String> {
    let minimum = config
        .minimum
        .as_deref()
        .map(|value| parse(kind, value))
        .transpose()?;
    let maximum = config
        .maximum
        .as_deref()
        .map(|value| parse(kind, value))
        .transpose()?;
    if let (Some(minimum), Some(maximum)) = (&minimum, &maximum)
        && compare(minimum, maximum)? == Ordering::Greater
    {
        return Err("Minimum must not exceed maximum.".into());
    }
    if let Some(step) = config.step.as_deref() {
        let step = parse(kind, step)?;
        if compare(&step, &zero(kind)?)? != Ordering::Greater {
            return Err("Step must be positive.".into());
        }
    }
    Ok(())
}

pub(crate) fn validate_value(
    kind: &InputKind,
    config: &InputConfig,
    value: &str,
) -> Result<(), String> {
    validate_config(kind, config)?;
    let value = parse(kind, value)?;
    if let Some(minimum) = config.minimum.as_deref()
        && compare(&value, &parse(kind, minimum)?)? == Ordering::Less
    {
        return Err("Value is below the configured minimum.".into());
    }
    if let Some(maximum) = config.maximum.as_deref()
        && compare(&value, &parse(kind, maximum)?)? == Ordering::Greater
    {
        return Err("Value is above the configured maximum.".into());
    }
    Ok(())
}

pub(crate) fn project(
    descriptor: LiteralInput,
    config: InputConfig,
) -> Result<InputProjection, String> {
    validate_config(&descriptor.kind, &config)?;
    let current_error = validate_value(&descriptor.kind, &config, &descriptor.value).err();
    let slider_available = match (&descriptor.kind, &config.minimum, &config.maximum) {
        (InputKind::Scalar | InputKind::Quantity { .. }, Some(min), Some(max)) => {
            matches!((parse(&descriptor.kind, min), parse(&descriptor.kind, max)), (Ok(Number::Float(a)), Ok(Number::Float(b))) if a < b)
        }
        (InputKind::Integer(mode), Some(min), Some(max)) => {
            match (parse(&descriptor.kind, min), parse(&descriptor.kind, max)) {
                (Ok(Number::Integer(a)), Ok(Number::Integer(b))) => {
                    let safe = |v: TypedInteger| {
                        if mode.is_signed() {
                            v.as_i128()
                                .is_some_and(|n| n.unsigned_abs() <= (1u128 << 53))
                        } else {
                            v.bit_pattern() <= (1u128 << 53)
                        }
                    };
                    safe(a)
                        && safe(b)
                        && compare(&Number::Integer(a), &Number::Integer(b)).ok()
                            == Some(Ordering::Less)
                }
                _ => false,
            }
        }
        _ => false,
    };
    Ok(InputProjection {
        descriptor,
        config,
        current_error,
        slider_available,
    })
}

/// Project an input draft onto original source, preserving all nonliteral bytes.
pub(crate) fn replace_literal(
    source: &str,
    descriptor: &LiteralInput,
    config: &InputConfig,
    value: &str,
) -> Result<String, String> {
    if source.get(descriptor.statement_range.clone()) != Some(descriptor.source_line.as_str())
        || source.get(descriptor.value_range.clone()) != Some(descriptor.value.as_str())
    {
        return Err("This input changed; review it before applying.".into());
    }
    validate_value(&descriptor.kind, config, value)?;
    let mut rendered = value.to_owned();
    if matches!(descriptor.kind, InputKind::ExactInteger) && config.anchor.is_some() {
        let original = descriptor.value.trim_start_matches(['+', '-']);
        let prefix = original.get(..2).unwrap_or("");
        let radix = match prefix {
            "0x" | "0X" => 16,
            "0b" | "0B" => 2,
            "0o" | "0O" => 8,
            _ => 10,
        };
        if radix != 10 {
            let Number::ExactInteger(number) = parse(&descriptor.kind, value)? else {
                return Err("Enter an integer literal.".into());
            };
            let mut digits = number.to_str_radix(radix);
            let negative = digits.starts_with('-');
            if negative {
                digits.remove(0);
            }
            if prefix.chars().any(char::is_uppercase) {
                digits.make_ascii_uppercase();
            }
            rendered = format!("{}{prefix}{digits}", if negative { "-" } else { "" });
        }
    }
    let size = source
        .len()
        .checked_sub(descriptor.value_range.len())
        .and_then(|size| size.checked_add(rendered.len()))
        .ok_or("Input is too large.")?;
    if size > super::limits::ABSOLUTE_BYTES {
        return Err("Input is too large.".into());
    }
    let mut statement = descriptor.source_line.clone();
    let local = descriptor.value_range.start - descriptor.statement_range.start
        ..descriptor.value_range.end - descriptor.statement_range.start;
    statement.replace_range(local.clone(), &rendered);
    let valid = super::document::literal_inputs(&statement)
        .into_iter()
        .any(|input| {
            input.line == 0
                && input.name == descriptor.name
                && (input.kind == descriptor.kind || config.anchor.is_none())
                && input.value == rendered
                && input.value_range.start == local.start
        });
    if !valid {
        return Err("Enter a valid value for this input.".into());
    }
    let mut replacement = source.to_owned();
    replacement.replace_range(descriptor.value_range.clone(), &rendered);
    Ok(replacement)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured(kind: &InputKind, minimum: &str, maximum: &str, step: &str) -> InputConfig {
        InputConfig {
            minimum: Some(minimum.into()),
            maximum: Some(maximum.into()),
            step: Some(step.into()),
            anchor: Some(kind.anchor()),
            ..Default::default()
        }
    }

    #[test]
    fn typed_constraints_are_exact_and_never_clamp() {
        let source = "n = u128(340282366920938463463374607431768211455)\nd = decimal(\"0.1\")\nq = 5 m\nraw = 0x20000000000000";
        let inputs = super::super::document::literal_inputs(source);
        assert_eq!(inputs.len(), 4);
        let integer = configured(
            &inputs[0].kind,
            "0",
            "340282366920938463463374607431768211455",
            "1",
        );
        assert!(
            validate_value(
                &inputs[0].kind,
                &integer,
                "340282366920938463463374607431768211455"
            )
            .is_ok()
        );
        assert!(
            validate_value(
                &inputs[0].kind,
                &integer,
                "340282366920938463463374607431768211456"
            )
            .is_err()
        );
        assert!(validate_value(&inputs[0].kind, &integer, "-1").is_err());
        let decimal = configured(&inputs[1].kind, "0.1", "0.3", "0.01");
        assert!(validate_value(&inputs[1].kind, &decimal, "0.30000000000000000001").is_err());
        assert!(validate_value(&inputs[1].kind, &decimal, "0.2").is_ok());
        let quantity = configured(&inputs[2].kind, "1", "10", "0.5");
        assert!(validate_value(&inputs[2].kind, &quantity, "11").is_err());
        assert!(
            replace_literal(source, &inputs[2], &quantity, "6")
                .unwrap()
                .contains("q = 6 m")
        );
        let raw = configured(&inputs[3].kind, "9007199254740992", "9007199254740994", "1");
        assert!(validate_value(&inputs[3].kind, &raw, "9007199254740995").is_err());
        assert!(validate_value(&inputs[3].kind, &raw, "9007199254740993").is_ok());
        let raw_valid = configured(&inputs[3].kind, "0", "9007199254740992", "1");
        assert!(
            replace_literal(source, &inputs[3], &raw_valid, "9007199254740991")
                .unwrap()
                .contains("raw = 0x1fffffffffffff")
        );
        assert!(replace_literal(source, &inputs[3], &raw, "9007199254740993").is_err());
        let mut bad = decimal.clone();
        bad.step = Some("0".into());
        assert!(validate_config(&inputs[1].kind, &bad).is_err());
    }

    #[test]
    fn projection_preserves_multiline_statement_and_rejects_type_changes() {
        let source = "{α} = decimal( // keep\r\n  \"0.10\") # note\r\nother = 2";
        let descriptor = super::super::document::literal_inputs(source).remove(0);
        let config = configured(&descriptor.kind, "0", "1", "0.1");
        let projected = project(descriptor.clone(), config.clone()).unwrap();
        assert!(projected.current_error.is_none());
        assert!(!projected.slider_available);
        assert_eq!(
            replace_literal(source, &descriptor, &config, "0.25").unwrap(),
            source.replace("0.10", "0.25")
        );
        assert!(
            replace_literal(
                &source.replace("// keep", "// changed"),
                &descriptor,
                &config,
                "0.25"
            )
            .is_err()
        );
        assert!(
            validate_config(
                &InputKind::Quantity { unit: "cm".into() },
                &configured(&InputKind::Quantity { unit: "m".into() }, "0", "5", "1")
            )
            .is_err()
        );
    }

    #[test]
    fn default_projection_accepts_long_valid_exact_decimal() {
        let value = format!("0.{}1", "0".repeat(300));
        let source = format!("x = decimal(\"{value}\")");
        let descriptor = super::super::document::literal_inputs(&source).remove(0);
        assert!(
            project(descriptor.clone(), InputConfig::default())
                .unwrap()
                .current_error
                .is_none()
        );
        assert_eq!(
            replace_literal(&source, &descriptor, &InputConfig::default(), &value).unwrap(),
            source
        );
    }
}
