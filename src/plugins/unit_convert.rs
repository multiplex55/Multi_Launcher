use super::base_convert;
use crate::actions::Action;
use crate::common::number_format::format_number;
use crate::plugin::Plugin;
use crate::unit_conversion::{self, ConversionError, ConversionOutcome};

pub struct UnitConvertPlugin;

fn conversion_query(query: &str) -> Option<&str> {
    let trimmed = query.trim_start();
    let split = trimmed.find(char::is_whitespace)?;
    let (prefix, rest) = trimmed.split_at(split);
    if !prefix.eq_ignore_ascii_case("conv") && !prefix.eq_ignore_ascii_case("convert") {
        return None;
    }
    Some(rest.trim_start())
}

fn error_action(error: ConversionError) -> Action {
    let label = match error {
        ConversionError::InvalidExpression { .. } => "Invalid conversion expression".to_owned(),
        ConversionError::InvalidNumber { expression } => {
            format!("Invalid number: {}", expression.trim())
        }
        ConversionError::UnknownUnit { expression } if expression.trim().is_empty() => {
            "Unknown unit".to_owned()
        }
        ConversionError::UnknownUnit { expression } => {
            format!("Unknown unit: {}", expression.trim())
        }
        ConversionError::IncompatibleUnits { source, target } => format!(
            "Cannot convert {} to {}",
            source.category().display_name().to_lowercase(),
            target.category().display_name().to_lowercase()
        ),
        ConversionError::NonlinearCompound { unit } => format!(
            "Compound quantities are not supported for {}",
            unit.category().display_name().to_lowercase()
        ),
        ConversionError::OutOfRange { .. } => "Value is outside the supported range".to_owned(),
    };
    Action {
        label: label.clone(),
        desc: "Unit convert".into(),
        action: format!("noop:{label}"),
        args: None,
    }
}

fn outcome_action(outcome: ConversionOutcome) -> Option<Action> {
    let formatted = format_number(outcome.value)?;
    let description = if outcome.is_approximate() {
        "Approximate unit conversion"
    } else {
        "Unit convert"
    };
    let destination = outcome.request.destination_expression;
    let label = format!(
        "{} = {} {}",
        outcome.request.source_expression, formatted, destination
    );
    Some(Action {
        label,
        desc: description.into(),
        action: format!("clipboard:{formatted} {destination}"),
        args: None,
    })
}

impl Plugin for UnitConvertPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let Some(rest) = conversion_query(query) else {
            return Vec::new();
        };
        if rest.is_empty() || base_convert::recognizes_conversion(rest) {
            return Vec::new();
        }

        match unit_conversion::evaluate_conversion(rest) {
            Ok(outcome) => outcome_action(outcome).into_iter().collect(),
            Err(error) if unit_conversion::contains_unit_alias(rest) => {
                vec![error_action(error)]
            }
            Err(_) => Vec::new(),
        }
    }

    fn name(&self) -> &str {
        "unit_convert"
    }

    fn description(&self) -> &str {
        "Convert between units (prefix: `conv` or `convert`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "conv".into(),
                desc: "Unit convert".into(),
                action: "query:conv ".into(),
                args: None,
            },
            Action {
                label: "convert".into(),
                desc: "Unit convert".into(),
                action: "query:convert ".into(),
                args: None,
            },
        ]
    }
}
