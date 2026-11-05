//! Bounded dimensional arithmetic for Calculation Pad.

use crate::common::number_format::format_number;
use crate::unit_conversion::{self, Approximation, Category, Unit};
use std::collections::HashMap;
use std::fmt;

use super::document::CalculationValue;

const MAX_TOKENS: usize = 256;
const MAX_NESTING: usize = 32;
const MAX_UNIT_POWER: i8 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantityError {
    InvalidSyntax,
    TooComplex,
    UnknownUnit(String),
    IncompatibleDimensions,
    NonLinearUnit(Unit),
    InvalidExponent,
    DivisionByZero,
    OutOfRange,
    ExpectedNumericValue(String),
}

impl fmt::Display for QuantityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSyntax => formatter.write_str("invalid quantity expression"),
            Self::TooComplex => formatter.write_str("quantity expression exceeds its work limit"),
            Self::UnknownUnit(unit) => write!(formatter, "unknown unit: {unit}"),
            Self::IncompatibleDimensions => {
                formatter.write_str("quantity dimensions are incompatible")
            }
            Self::NonLinearUnit(unit) => write!(
                formatter,
                "{} cannot be used in dimensional arithmetic",
                unit.symbol()
            ),
            Self::InvalidExponent => {
                write!(
                    formatter,
                    "unit and quantity powers must be integers from -{MAX_UNIT_POWER} to {MAX_UNIT_POWER}"
                )
            }
            Self::DivisionByZero => formatter.write_str("quantity division by zero"),
            Self::OutOfRange => formatter.write_str("quantity result is outside the finite range"),
            Self::ExpectedNumericValue(name) => {
                write!(formatter, "{name} is not a scalar or linear quantity")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Dimension(Vec<(Category, i8)>);

impl Dimension {
    fn from_unit(unit: Unit, power: i8) -> Result<Self, QuantityError> {
        let mut dimension = Self::default();
        let category = unit.category();
        match category {
            Category::Length => dimension.add_power(Category::Length, power)?,
            Category::Duration => dimension.add_power(Category::Duration, power)?,
            Category::Area => dimension.add_power(
                Category::Length,
                power.checked_mul(2).ok_or(QuantityError::InvalidExponent)?,
            )?,
            Category::Speed => {
                dimension.add_power(Category::Length, power)?;
                dimension.add_power(
                    Category::Duration,
                    power.checked_neg().ok_or(QuantityError::InvalidExponent)?,
                )?;
            }
            Category::DataRate => {
                dimension.add_power(Category::Data, power)?;
                dimension.add_power(
                    Category::Duration,
                    power.checked_neg().ok_or(QuantityError::InvalidExponent)?,
                )?;
            }
            other => dimension.add_power(other, power)?,
        }
        Ok(dimension)
    }

    fn add_power(&mut self, category: Category, amount: i8) -> Result<(), QuantityError> {
        let old = self
            .0
            .iter()
            .find_map(|(candidate, power)| (*candidate == category).then_some(*power))
            .unwrap_or(0);
        let new = old
            .checked_add(amount)
            .filter(|power| power.unsigned_abs() <= MAX_UNIT_POWER as u8)
            .ok_or(QuantityError::InvalidExponent)?;
        self.0.retain(|(candidate, _)| *candidate != category);
        if new != 0 {
            self.0.push((category, new));
        }
        self.0
            .sort_by_key(|(candidate, _)| category_order(*candidate));
        Ok(())
    }

    fn combine(&self, other: &Self, sign: i8) -> Result<Self, QuantityError> {
        let mut combined = self.clone();
        for (category, power) in &other.0 {
            combined.add_power(
                *category,
                power
                    .checked_mul(sign)
                    .ok_or(QuantityError::InvalidExponent)?,
            )?;
        }
        Ok(combined)
    }

    fn multiplied(&self, power: i32) -> Result<Self, QuantityError> {
        let power = i8::try_from(power).map_err(|_| QuantityError::InvalidExponent)?;
        let mut result = Self::default();
        for (category, exponent) in &self.0 {
            result.add_power(
                *category,
                exponent
                    .checked_mul(power)
                    .ok_or(QuantityError::InvalidExponent)?,
            )?;
        }
        Ok(result)
    }
}

fn category_order(category: Category) -> usize {
    Category::ALL
        .iter()
        .position(|candidate| *candidate == category)
        .unwrap_or(usize::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnitPower {
    unit: Unit,
    power: i8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct UnitProduct {
    terms: Vec<UnitPower>,
    approximations: Vec<Approximation>,
}

impl UnitProduct {
    fn single(unit: Unit) -> Self {
        Self {
            terms: vec![UnitPower { unit, power: 1 }],
            approximations: unit.approximation().into_iter().collect(),
        }
    }

    fn add(&mut self, unit: Unit, power: i8) -> Result<(), QuantityError> {
        if unit.linear_factor().is_none() {
            return Err(QuantityError::NonLinearUnit(unit));
        }
        if let Some(approximation) = unit.approximation()
            && !self.approximations.contains(&approximation)
        {
            self.approximations.push(approximation);
        }
        let old = self
            .terms
            .iter()
            .find_map(|term| (term.unit == unit).then_some(term.power))
            .unwrap_or(0);
        let new = old
            .checked_add(power)
            .filter(|value| value.unsigned_abs() <= MAX_UNIT_POWER as u8)
            .ok_or(QuantityError::InvalidExponent)?;
        self.terms.retain(|term| term.unit != unit);
        if new != 0 {
            self.terms.push(UnitPower { unit, power: new });
        }
        self.terms
            .sort_by(|left, right| left.unit.symbol().cmp(right.unit.symbol()));
        Ok(())
    }

    fn from_catalog_alias(unit: Unit, alias: &str, power: i8) -> Result<Self, QuantityError> {
        if unit.linear_factor().is_none() {
            return Err(QuantityError::NonLinearUnit(unit));
        }
        if alias.contains('/') && unit.category() == Category::DataRate && power == 1 {
            let product = Self::single(unit);
            return Ok(product);
        }
        let mut product = Self::default();
        if matches!(unit.category(), Category::Speed | Category::DataRate)
            && let Some((numerator, denominator)) = alias.split_once('/')
            && let (Some(numerator), Some(denominator)) = (
                unit_conversion::unit_by_alias(numerator.trim()),
                unit_conversion::unit_by_alias(denominator.trim()),
            )
            && numerator.linear_factor().is_some()
            && denominator.linear_factor().is_some()
        {
            product.add(numerator, 1)?;
            product.add(
                denominator,
                power.checked_neg().ok_or(QuantityError::InvalidExponent)?,
            )?;
            return Ok(product);
        }
        product.add(unit, power)?;
        Ok(product)
    }

    fn combine(&self, other: &Self, sign: i8) -> Result<Self, QuantityError> {
        let mut combined = self.clone();
        for term in &other.terms {
            combined.add(
                term.unit,
                term.power
                    .checked_mul(sign)
                    .ok_or(QuantityError::InvalidExponent)?,
            )?;
        }
        for approximation in &other.approximations {
            if !combined.approximations.contains(approximation) {
                combined.approximations.push(*approximation);
            }
        }
        Ok(combined)
    }

    fn powered(&self, exponent: i32) -> Result<Self, QuantityError> {
        let exponent = i8::try_from(exponent).map_err(|_| QuantityError::InvalidExponent)?;
        let mut result = Self {
            terms: Vec::new(),
            approximations: self.approximations.clone(),
        };
        for term in &self.terms {
            result.add(
                term.unit,
                term.power
                    .checked_mul(exponent)
                    .ok_or(QuantityError::InvalidExponent)?,
            )?;
        }
        Ok(result)
    }

    fn factor(&self) -> Result<f64, QuantityError> {
        let mut factor = 1.0;
        for term in &self.terms {
            let base = term
                .unit
                .linear_factor()
                .ok_or(QuantityError::NonLinearUnit(term.unit))?;
            factor *= base.powi(i32::from(term.power));
            if !factor.is_finite() || factor == 0.0 {
                return Err(QuantityError::OutOfRange);
            }
        }
        Ok(factor)
    }

    fn dimension(&self) -> Result<Dimension, QuantityError> {
        let mut dimension = Dimension::default();
        for term in &self.terms {
            let unit_dimension = Dimension::from_unit(term.unit, term.power)?;
            dimension = dimension.combine(&unit_dimension, 1)?;
        }
        Ok(dimension)
    }

    fn formatted_unit(&self) -> String {
        if self.terms.is_empty() {
            return String::new();
        }
        let numerator = self
            .terms
            .iter()
            .filter(|term| term.power > 0)
            .map(|term| format_unit_power(term.unit, term.power.unsigned_abs()))
            .collect::<Vec<_>>();
        let denominator = self
            .terms
            .iter()
            .filter(|term| term.power < 0)
            .map(|term| format_denominator_power(term.unit, term.power.unsigned_abs()))
            .collect::<Vec<_>>();
        let numerator = if numerator.is_empty() {
            "1".to_owned()
        } else {
            numerator.join("·")
        };
        match denominator.len() {
            0 => numerator,
            1 => format!("{numerator}/{}", denominator[0]),
            _ => format!("{numerator}/({})", denominator.join("·")),
        }
    }
}

fn format_unit_power(unit: Unit, power: u8) -> String {
    let symbol = unit.symbol();
    if power == 1 {
        return symbol.to_owned();
    }
    let superscript = match power {
        2 => "²".to_owned(),
        3 => "³".to_owned(),
        value => format!("^{value}"),
    };
    if symbol.contains('/') {
        format!("({symbol}){superscript}")
    } else {
        format!("{symbol}{superscript}")
    }
}

fn format_denominator_power(unit: Unit, power: u8) -> String {
    let formatted = format_unit_power(unit, power);
    if power == 1 && unit.symbol().contains('/') {
        format!("({formatted})")
    } else {
        formatted
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalculatedQuantity {
    base_value: f64,
    dimension: Dimension,
    display_units: UnitProduct,
    approximations: Vec<Approximation>,
}

impl CalculatedQuantity {
    pub(crate) fn formatted_result(&self) -> Option<String> {
        let factor = self.display_units.factor().ok()?;
        let value = self.base_value / factor;
        if !value.is_finite() || (value == 0.0 && self.base_value != 0.0) {
            return None;
        }
        let formatted_value = format_number(value)?;
        let unit = self.display_units.formatted_unit();
        if unit.is_empty() {
            Some(formatted_value)
        } else {
            Some(format!("{formatted_value} {unit}"))
        }
    }

    pub(crate) fn is_approximate(&self) -> bool {
        !self.approximations().is_empty()
    }

    pub(crate) fn approximations(&self) -> &[Approximation] {
        &self.approximations
    }
}

#[derive(Debug)]
pub(crate) struct ParseAttempt {
    pub expression: Option<QuantityExpression>,
    pub error: Option<QuantityError>,
    pub references: Vec<String>,
    pub has_unit: bool,
}

#[derive(Debug)]
pub(crate) struct QuantityExpression {
    root: Expr,
}

impl QuantityExpression {
    pub(crate) fn evaluate(
        &self,
        bindings: &HashMap<String, CalculationValue>,
        force_quantity: bool,
    ) -> Result<CalculationValue, QuantityError> {
        let value = evaluate_expr(&self.root, bindings)?;
        match value {
            EvalValue::Scalar(value) if !force_quantity => Ok(CalculationValue::Scalar(value)),
            EvalValue::Scalar(value) => {
                let quantity = CalculatedQuantity {
                    base_value: value,
                    dimension: Dimension::default(),
                    display_units: UnitProduct::default(),
                    approximations: Vec::new(),
                };
                quantity
                    .formatted_result()
                    .ok_or(QuantityError::OutOfRange)?;
                Ok(CalculationValue::DimensionalQuantity(quantity))
            }
            EvalValue::Quantity(value) => {
                value.formatted_result().ok_or(QuantityError::OutOfRange)?;
                Ok(CalculationValue::DimensionalQuantity(value))
            }
        }
    }
}

#[derive(Debug)]
enum Expr {
    Number(f64),
    Reference(String),
    AttachUnit(Box<Expr>, UnitProduct),
    Unary {
        operator: UnaryOperator,
        operand: Box<Expr>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy)]
enum UnaryOperator {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Copy)]
enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

#[derive(Debug, Clone)]
enum EvalValue {
    Scalar(f64),
    Quantity(CalculatedQuantity),
}

pub(crate) fn parse_expression(source: &str) -> ParseAttempt {
    let mut parser = Parser::new(source);
    let parsed = parser.parse_expression(0);
    let result = parsed.and_then(|expression| {
        parser.skip_whitespace();
        if parser.cursor != source.len() {
            return Err(QuantityError::InvalidSyntax);
        }
        Ok(expression)
    });
    let (expression, error) = match result {
        Ok(root) => (Some(QuantityExpression { root }), None),
        Err(error) => (None, Some(error)),
    };
    ParseAttempt {
        expression,
        error,
        references: parser.references,
        has_unit: parser.has_unit,
    }
}

pub(crate) fn parse_unit_target(source: &str) -> Result<UnitProduct, QuantityError> {
    let trimmed = source.trim();
    if let Some(unit) = unit_conversion::unit_by_alias(trimmed) {
        if unit.linear_factor().is_none() {
            return Err(QuantityError::NonLinearUnit(unit));
        }
        let product = UnitProduct::single(unit);
        product.factor()?;
        return Ok(product);
    }

    let mut parser = Parser::new(trimmed);
    let product = parser.parse_unit_product(true)?;
    parser.skip_whitespace();
    if parser.cursor != trimmed.len() {
        return Err(QuantityError::UnknownUnit(trimmed.to_owned()));
    }
    Ok(product)
}

pub(crate) fn convert_to_target(
    quantity: &CalculatedQuantity,
    target: &UnitProduct,
) -> Result<CalculatedQuantity, QuantityError> {
    if target.dimension()? != quantity.dimension {
        return Err(QuantityError::IncompatibleDimensions);
    }
    let mut approximations = quantity.approximations.clone();
    for approximation in &target.approximations {
        if !approximations.contains(approximation) {
            approximations.push(*approximation);
        }
    }
    let converted = CalculatedQuantity {
        base_value: quantity.base_value,
        dimension: quantity.dimension.clone(),
        display_units: target.clone(),
        approximations,
    };
    converted
        .formatted_result()
        .ok_or(QuantityError::OutOfRange)?;
    Ok(converted)
}

pub(crate) fn convert_to_unit(
    quantity: &CalculatedQuantity,
    target: Unit,
) -> Result<CalculatedQuantity, QuantityError> {
    convert_to_target(quantity, &UnitProduct::single(target))
}

struct Parser<'a> {
    source: &'a str,
    cursor: usize,
    token_count: usize,
    recursion: usize,
    references: Vec<String>,
    has_unit: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            cursor: 0,
            token_count: 0,
            recursion: 0,
            references: Vec::new(),
            has_unit: false,
        }
    }

    fn parse_expression(&mut self, minimum_precedence: u8) -> Result<Expr, QuantityError> {
        self.recursion += 1;
        if self.recursion > MAX_NESTING {
            self.recursion -= 1;
            return Err(QuantityError::TooComplex);
        }
        let result = self.parse_expression_inner(minimum_precedence);
        self.recursion -= 1;
        result
    }

    fn parse_expression_inner(&mut self, minimum_precedence: u8) -> Result<Expr, QuantityError> {
        let mut left = self.parse_prefix()?;
        loop {
            self.skip_whitespace();
            let Some((operator, precedence, right_associative)) = self.peek_operator() else {
                break;
            };
            if precedence < minimum_precedence {
                break;
            }
            self.bump_token()?;
            self.cursor += operator.symbol().len();
            let right_precedence = if right_associative {
                precedence
            } else {
                precedence + 1
            };
            let right = self.parse_expression(right_precedence)?;
            left = Expr::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr, QuantityError> {
        self.skip_whitespace();
        let Some(character) = self.source[self.cursor..].chars().next() else {
            return Err(QuantityError::InvalidSyntax);
        };
        if character == '+' || character == '-' {
            self.bump_token()?;
            self.cursor += character.len_utf8();
            let operand = self.parse_expression(7)?;
            return Ok(Expr::Unary {
                operator: if character == '+' {
                    UnaryOperator::Positive
                } else {
                    UnaryOperator::Negative
                },
                operand: Box::new(operand),
            });
        }

        let mut primary = if character == '(' {
            self.bump_token()?;
            self.cursor += 1;
            let expression = self.parse_expression(0)?;
            self.skip_whitespace();
            if !self.source[self.cursor..].starts_with(')') {
                return Err(QuantityError::InvalidSyntax);
            }
            self.bump_token()?;
            self.cursor += 1;
            expression
        } else if starts_number(character) {
            self.bump_token()?;
            let (value, consumed) =
                unit_conversion::parse_quantity_number_prefix(&self.source[self.cursor..])
                    .map_err(|_| QuantityError::OutOfRange)?;
            if consumed == 0 || !value.is_finite() {
                return Err(QuantityError::OutOfRange);
            }
            self.cursor += consumed;
            Expr::Number(value)
        } else if character == '{' {
            self.bump_token()?;
            self.cursor += 1;
            let start = self.cursor;
            let Some(end) = self.source[start..].find('}') else {
                return Err(QuantityError::InvalidSyntax);
            };
            let name = self.source[start..start + end].to_owned();
            if name.is_empty() || name.contains(['{', '\n', '\r']) {
                return Err(QuantityError::InvalidSyntax);
            }
            self.cursor = start + end + 1;
            self.record_reference(name.clone());
            Expr::Reference(name)
        } else if is_identifier_start(character) {
            self.bump_token()?;
            let start = self.cursor;
            self.cursor += character.len_utf8();
            while let Some(next) = self.source[self.cursor..].chars().next()
                && is_identifier_continue(next)
            {
                self.cursor += next.len_utf8();
            }
            let name = self.source[start..self.cursor].to_owned();
            if let Some(value) = scalar_constant(&name) {
                Expr::Number(value)
            } else {
                self.record_reference(name.clone());
                Expr::Reference(name)
            }
        } else {
            return Err(QuantityError::InvalidSyntax);
        };

        if let Some(units) = self.parse_unit_suffix()? {
            self.has_unit = true;
            primary = Expr::AttachUnit(Box::new(primary), units);
        }
        Ok(primary)
    }

    fn parse_unit_suffix(&mut self) -> Result<Option<UnitProduct>, QuantityError> {
        let initial_cursor = self.cursor;
        self.skip_whitespace();
        let alias_start = self.cursor;
        let Some((unit, consumed)) =
            unit_conversion::match_unit_prefix_for_quantity(&self.source[self.cursor..])
        else {
            self.cursor = initial_cursor;
            return Ok(None);
        };
        self.has_unit = true;
        self.bump_token()?;
        self.cursor += consumed;
        let alias = self.source[alias_start..self.cursor].trim();
        let power = self.parse_attached_unit_power()?.unwrap_or(1);
        let mut product = UnitProduct::from_catalog_alias(unit, alias, power)?;

        // Adjacent slash notation is unambiguous as a compound unit. A spaced
        // slash stays available to the expression parser, so `distance / s`
        // can continue to refer to a variable named `s`.
        loop {
            let slash_at = self.cursor;
            if !self.source[slash_at..].starts_with('/') {
                break;
            }
            let unit_start = slash_at + 1;
            let Some((denominator, consumed)) =
                unit_conversion::match_unit_prefix_for_quantity(&self.source[unit_start..])
            else {
                break;
            };
            self.bump_token()?;
            self.cursor += 1;
            self.bump_token()?;
            self.cursor += consumed;
            let power = self.parse_attached_unit_power()?.unwrap_or(1);
            product.add(
                denominator,
                power.checked_neg().ok_or(QuantityError::InvalidExponent)?,
            )?;
        }
        Ok(Some(product))
    }

    fn parse_attached_unit_power(&mut self) -> Result<Option<i8>, QuantityError> {
        let power_start = self.cursor;
        if !self.source[self.cursor..].starts_with('^') {
            return Ok(None);
        }
        let mut cursor = self.cursor + 1;
        let negative = self.source[cursor..].starts_with('-');
        let positive = self.source[cursor..].starts_with('+');
        if negative || positive {
            cursor += 1;
        }
        let digit_start = cursor;
        while self
            .source
            .as_bytes()
            .get(cursor)
            .is_some_and(u8::is_ascii_digit)
        {
            cursor += 1;
        }
        if digit_start == cursor {
            return Ok(None);
        }
        let magnitude: i16 = self.source[digit_start..cursor]
            .parse()
            .map_err(|_| QuantityError::InvalidExponent)?;
        let signed = if negative { -magnitude } else { magnitude };
        if signed.unsigned_abs() > MAX_UNIT_POWER as u16 {
            return Err(QuantityError::InvalidExponent);
        }
        self.bump_token()?;
        self.cursor = cursor;
        if self.cursor == power_start {
            return Err(QuantityError::InvalidSyntax);
        }
        Ok(Some(signed as i8))
    }

    fn parse_unit_product(
        &mut self,
        allow_spaced_separators: bool,
    ) -> Result<UnitProduct, QuantityError> {
        self.skip_whitespace();
        let start = self.cursor;
        let alias_start = self.cursor;
        let Some((first, consumed)) =
            unit_conversion::match_unit_prefix_for_quantity(&self.source[self.cursor..])
        else {
            return Err(QuantityError::UnknownUnit(
                self.source[start..].trim().to_owned(),
            ));
        };
        self.bump_token()?;
        self.cursor += consumed;
        let alias = self.source[alias_start..self.cursor].trim();
        if allow_spaced_separators {
            self.skip_whitespace();
        }
        let power = self
            .parse_target_unit_power(allow_spaced_separators)?
            .unwrap_or(1);
        let mut product = UnitProduct::from_catalog_alias(first, alias, power)?;
        loop {
            let separator_start = self.cursor;
            if allow_spaced_separators {
                self.skip_whitespace();
            }
            let separator = if self.source[self.cursor..].starts_with('/') {
                Some(-1)
            } else if allow_spaced_separators && self.source[self.cursor..].starts_with('*') {
                Some(1)
            } else {
                None
            };
            let Some(sign) = separator else {
                if !allow_spaced_separators {
                    self.cursor = separator_start;
                }
                break;
            };
            self.bump_token()?;
            self.cursor += 1;
            self.skip_whitespace();
            let unit_start = self.cursor;
            let Some((unit, consumed)) =
                unit_conversion::match_unit_prefix_for_quantity(&self.source[self.cursor..])
            else {
                return Err(QuantityError::UnknownUnit(
                    self.source[unit_start..].trim().to_owned(),
                ));
            };
            self.bump_token()?;
            self.cursor += consumed;
            let mut power = 1;
            if allow_spaced_separators {
                self.skip_whitespace();
            }
            if let Some(unit_power) = self.parse_target_unit_power(allow_spaced_separators)? {
                power = unit_power;
            }
            product.add(
                unit,
                power
                    .checked_mul(sign)
                    .ok_or(QuantityError::InvalidExponent)?,
            )?;
        }
        Ok(product)
    }

    fn parse_target_unit_power(&mut self, allow_spaces: bool) -> Result<Option<i8>, QuantityError> {
        let old_cursor = self.cursor;
        if allow_spaces {
            self.skip_whitespace();
        }
        if !self.source[self.cursor..].starts_with('^') {
            self.cursor = old_cursor;
            return Ok(None);
        }
        self.cursor += 1;
        self.skip_whitespace();
        let negative = self.source[self.cursor..].starts_with('-');
        if negative || self.source[self.cursor..].starts_with('+') {
            self.cursor += 1;
        }
        let start = self.cursor;
        while self
            .source
            .as_bytes()
            .get(self.cursor)
            .is_some_and(u8::is_ascii_digit)
        {
            self.cursor += 1;
        }
        if start == self.cursor {
            return Err(QuantityError::InvalidExponent);
        }
        let magnitude: i16 = self.source[start..self.cursor]
            .parse()
            .map_err(|_| QuantityError::InvalidExponent)?;
        let power = if negative { -magnitude } else { magnitude };
        if power.unsigned_abs() > MAX_UNIT_POWER as u16 {
            return Err(QuantityError::InvalidExponent);
        }
        self.bump_token()?;
        Ok(Some(power as i8))
    }

    fn peek_operator(&self) -> Option<(BinaryOperator, u8, bool)> {
        let remainder = &self.source[self.cursor..];
        if remainder.starts_with('+') {
            Some((BinaryOperator::Add, 5, false))
        } else if remainder.starts_with('-') {
            Some((BinaryOperator::Subtract, 5, false))
        } else if remainder.starts_with('*') {
            Some((BinaryOperator::Multiply, 6, false))
        } else if remainder.starts_with('/') {
            Some((BinaryOperator::Divide, 6, false))
        } else if remainder.starts_with('^') {
            Some((BinaryOperator::Power, 7, true))
        } else {
            None
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.source[self.cursor..].chars().next()
            && character.is_whitespace()
        {
            self.cursor += character.len_utf8();
        }
    }

    fn bump_token(&mut self) -> Result<(), QuantityError> {
        self.token_count += 1;
        if self.token_count > MAX_TOKENS {
            Err(QuantityError::TooComplex)
        } else {
            Ok(())
        }
    }

    fn record_reference(&mut self, name: String) {
        if !self.references.contains(&name) {
            self.references.push(name);
        }
    }
}

impl BinaryOperator {
    fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Power => "^",
        }
    }
}

fn starts_number(character: char) -> bool {
    character.is_ascii_digit() || matches!(character, '.' | '½' | '¼' | '¾')
}

fn is_identifier_start(character: char) -> bool {
    character == '_'
        || character.is_ascii_alphabetic()
        || matches!(character, 'α'..='ω' | 'Α'..='Ω')
}

fn is_identifier_continue(character: char) -> bool {
    is_identifier_start(character) || character.is_ascii_digit()
}

fn scalar_constant(name: &str) -> Option<f64> {
    match name {
        "PI" | "π" => Some(std::f64::consts::PI),
        "E" | "e" => Some(std::f64::consts::E),
        "TAU" | "τ" => Some(std::f64::consts::TAU),
        _ => None,
    }
}

fn evaluate_expr(
    expression: &Expr,
    bindings: &HashMap<String, CalculationValue>,
) -> Result<EvalValue, QuantityError> {
    let value = evaluate_expr_inner(expression, bindings)?;
    if let EvalValue::Quantity(quantity) = &value {
        quantity
            .formatted_result()
            .ok_or(QuantityError::OutOfRange)?;
    }
    Ok(value)
}

fn evaluate_expr_inner(
    expression: &Expr,
    bindings: &HashMap<String, CalculationValue>,
) -> Result<EvalValue, QuantityError> {
    match expression {
        Expr::Number(value) => Ok(EvalValue::Scalar(*value)),
        Expr::Reference(name) => match bindings.get(name) {
            Some(CalculationValue::Scalar(value)) => Ok(EvalValue::Scalar(*value)),
            Some(CalculationValue::DimensionalQuantity(value)) => {
                Ok(EvalValue::Quantity(value.clone()))
            }
            Some(CalculationValue::PhysicalQuantity {
                value,
                unit,
                approximations,
            }) => from_physical(*value, *unit, approximations),
            Some(_) => Err(QuantityError::ExpectedNumericValue(name.clone())),
            None => Err(QuantityError::ExpectedNumericValue(name.clone())),
        },
        Expr::AttachUnit(value, units) => {
            let value = evaluate_expr(value, bindings)?;
            let factor = units.factor()?;
            let dimension = units.dimension()?;
            let (base_value, approximations) = match value {
                EvalValue::Scalar(value) => (value, Vec::new()),
                EvalValue::Quantity(value) if value.dimension.0.is_empty() => {
                    (value.base_value, value.approximations)
                }
                EvalValue::Quantity(_) => return Err(QuantityError::IncompatibleDimensions),
            };
            let base_value = checked_product(base_value, factor)?;
            let mut approximations = approximations;
            add_unit_approximations(&mut approximations, units);
            Ok(EvalValue::Quantity(CalculatedQuantity {
                base_value,
                dimension,
                display_units: units.clone(),
                approximations,
            }))
        }
        Expr::Unary { operator, operand } => {
            let operand = evaluate_expr(operand, bindings)?;
            match (operator, operand) {
                (UnaryOperator::Positive, scalar @ EvalValue::Scalar(_)) => Ok(scalar),
                (UnaryOperator::Negative, EvalValue::Scalar(value)) => {
                    checked_value(-value).map(EvalValue::Scalar)
                }
                (UnaryOperator::Positive, quantity @ EvalValue::Quantity(_)) => Ok(quantity),
                (UnaryOperator::Negative, EvalValue::Quantity(mut quantity)) => {
                    quantity.base_value = checked_value(-quantity.base_value)?;
                    Ok(EvalValue::Quantity(quantity))
                }
            }
        }
        Expr::Binary {
            operator,
            left,
            right,
        } => {
            let left = evaluate_expr(left, bindings)?;
            let right = evaluate_expr(right, bindings)?;
            evaluate_binary(*operator, left, right)
        }
    }
}

fn from_physical(
    value: f64,
    unit: Unit,
    approximations: &[Approximation],
) -> Result<EvalValue, QuantityError> {
    let units = UnitProduct::single(unit);
    let base_value = checked_product(
        value,
        unit.linear_factor()
            .ok_or(QuantityError::NonLinearUnit(unit))?,
    )?;
    Ok(EvalValue::Quantity(CalculatedQuantity {
        base_value,
        dimension: units.dimension()?,
        display_units: units,
        approximations: approximations.to_vec(),
    }))
}

fn evaluate_binary(
    operator: BinaryOperator,
    left: EvalValue,
    right: EvalValue,
) -> Result<EvalValue, QuantityError> {
    match operator {
        BinaryOperator::Add | BinaryOperator::Subtract => {
            let (left_value, left_quantity) = unpack(left)?;
            let (right_value, right_quantity) = unpack(right)?;
            let left_dimension = left_quantity
                .as_ref()
                .map(|value| value.dimension.clone())
                .unwrap_or_default();
            let right_dimension = right_quantity
                .as_ref()
                .map(|value| value.dimension.clone())
                .unwrap_or_default();
            if left_dimension != right_dimension {
                return Err(QuantityError::IncompatibleDimensions);
            }
            let value = if matches!(operator, BinaryOperator::Add) {
                left_value + right_value
            } else {
                left_value - right_value
            };
            checked_value(value)?;
            if left_quantity.is_none() && right_quantity.is_none() {
                return Ok(EvalValue::Scalar(value));
            }
            let result_quantity = match (left_quantity, right_quantity) {
                (Some(mut left_quantity), Some(right_quantity)) => {
                    let mut approximations = left_quantity.approximations;
                    merge_approximations(&mut approximations, right_quantity.approximations);
                    left_quantity.approximations = approximations;
                    left_quantity
                }
                (Some(quantity), None) | (None, Some(quantity)) => quantity,
                (None, None) => unreachable!("scalar results returned above"),
            };
            Ok(EvalValue::Quantity(CalculatedQuantity {
                base_value: value,
                dimension: result_quantity.dimension,
                display_units: result_quantity.display_units,
                approximations: result_quantity.approximations,
            }))
        }
        BinaryOperator::Multiply | BinaryOperator::Divide => {
            evaluate_product(operator, left, right)
        }
        BinaryOperator::Power => evaluate_power(left, right),
    }
}

fn unpack(value: EvalValue) -> Result<(f64, Option<CalculatedQuantity>), QuantityError> {
    match value {
        EvalValue::Scalar(value) => Ok((value, None)),
        EvalValue::Quantity(value) => Ok((value.base_value, Some(value))),
    }
}

fn evaluate_product(
    operator: BinaryOperator,
    left: EvalValue,
    right: EvalValue,
) -> Result<EvalValue, QuantityError> {
    let divide = matches!(operator, BinaryOperator::Divide);
    let (left_value, left_quantity) = unpack(left)?;
    let (right_value, right_quantity) = unpack(right)?;
    if divide && right_value == 0.0 {
        return Err(QuantityError::DivisionByZero);
    }
    let base_value = if divide {
        left_value / right_value
    } else {
        left_value * right_value
    };
    if !base_value.is_finite()
        || (base_value == 0.0 && left_value != 0.0 && (divide || right_value != 0.0))
    {
        return Err(QuantityError::OutOfRange);
    }
    let right_sign = if divide { -1 } else { 1 };
    match (left_quantity, right_quantity) {
        (None, None) => Ok(EvalValue::Scalar(checked_value(base_value)?)),
        (Some(mut quantity), None) => {
            if divide {
                quantity.base_value = checked_value(base_value)?;
                quantity.dimension = quantity.dimension.multiplied(1)?;
            } else {
                quantity.base_value = checked_value(base_value)?;
            }
            Ok(EvalValue::Quantity(quantity))
        }
        (None, Some(quantity)) => {
            let exponent = if divide { -1 } else { 1 };
            let dimension = Dimension::default().combine(&quantity.dimension, exponent)?;
            let display_units =
                UnitProduct::default().combine(&quantity.display_units, exponent)?;
            Ok(EvalValue::Quantity(CalculatedQuantity {
                base_value: checked_value(base_value)?,
                dimension,
                display_units,
                approximations: quantity.approximations,
            }))
        }
        (Some(left), Some(right)) => {
            let dimension = left.dimension.combine(&right.dimension, right_sign)?;
            let display_units = left
                .display_units
                .combine(&right.display_units, right_sign)?;
            let mut approximations = left.approximations;
            merge_approximations(&mut approximations, right.approximations);
            Ok(EvalValue::Quantity(CalculatedQuantity {
                base_value: checked_value(base_value)?,
                dimension,
                display_units,
                approximations,
            }))
        }
    }
}

fn evaluate_power(left: EvalValue, right: EvalValue) -> Result<EvalValue, QuantityError> {
    let (exponent_value, exponent_quantity) = unpack(right)?;
    if exponent_quantity
        .as_ref()
        .is_some_and(|quantity| !quantity.dimension.0.is_empty())
    {
        return Err(QuantityError::IncompatibleDimensions);
    }
    if exponent_value.fract() != 0.0 || exponent_value.abs() > f64::from(MAX_UNIT_POWER) {
        return Err(QuantityError::InvalidExponent);
    }
    let exponent = exponent_value as i32;
    let (base, quantity) = unpack(left)?;
    let powered = base.powi(exponent);
    if !powered.is_finite() || (powered == 0.0 && base != 0.0 && exponent != 0) {
        return Err(QuantityError::OutOfRange);
    }
    match quantity {
        None if exponent_quantity.is_none() => Ok(EvalValue::Scalar(powered)),
        None => Ok(EvalValue::Quantity(CalculatedQuantity {
            base_value: powered,
            dimension: Dimension::default(),
            display_units: UnitProduct::default(),
            approximations: exponent_quantity
                .map(|quantity| quantity.approximations)
                .unwrap_or_default(),
        })),
        Some(mut quantity) => {
            quantity.base_value = powered;
            quantity.dimension = quantity.dimension.multiplied(exponent)?;
            quantity.display_units = quantity.display_units.powered(exponent)?;
            if let Some(exponent_quantity) = exponent_quantity {
                merge_approximations(
                    &mut quantity.approximations,
                    exponent_quantity.approximations,
                );
            }
            Ok(EvalValue::Quantity(quantity))
        }
    }
}

fn checked_product(left: f64, right: f64) -> Result<f64, QuantityError> {
    let value = left * right;
    if !value.is_finite() || (value == 0.0 && left != 0.0 && right != 0.0) {
        Err(QuantityError::OutOfRange)
    } else {
        Ok(value)
    }
}

fn checked_value(value: f64) -> Result<f64, QuantityError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(QuantityError::OutOfRange)
    }
}

fn add_unit_approximations(approximations: &mut Vec<Approximation>, units: &UnitProduct) {
    for approximation in &units.approximations {
        if !approximations.contains(approximation) {
            approximations.push(*approximation);
        }
    }
}

fn merge_approximations(target: &mut Vec<Approximation>, source: Vec<Approximation>) {
    for approximation in source {
        if !target.contains(&approximation) {
            target.push(approximation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_expression;
    use crate::calculation_pad::CalculationValue;
    use std::collections::HashMap;

    fn evaluate(source: &str) -> CalculationValue {
        let parsed = parse_expression(source);
        parsed
            .expression
            .expect("quantity expression should parse")
            .evaluate(&HashMap::new(), true)
            .expect("quantity expression should evaluate")
    }

    #[test]
    fn catalog_compound_units_keep_operator_precedence_and_grouped_symbols() {
        let precedence = evaluate("2/3^2 * (1 m)");
        let formatted = precedence.formatted_result().unwrap();
        assert_eq!(formatted, "0.222222 m");

        assert_eq!(
            evaluate("(2 MB/s)^2").formatted_result().as_deref(),
            Some("4 (MB/s)²")
        );
        assert_eq!(
            evaluate("1 m/s^2").formatted_result().as_deref(),
            Some("1 m/s²")
        );
        assert_eq!(
            evaluate("1/(1 MB/s)").formatted_result().as_deref(),
            Some("1 1/(MB/s)")
        );

        let cancelled_approximation = evaluate("1 month/month");
        assert_eq!(
            cancelled_approximation.formatted_result().as_deref(),
            Some("1")
        );
        assert!(cancelled_approximation.is_approximate());
    }
}
