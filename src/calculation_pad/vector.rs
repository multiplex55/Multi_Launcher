//! Bounded vector expressions composed with the ordinary scalar evaluator.
use super::document::{
    self, CalculationValue, DiagnosticKind, MAX_EXPRESSION_NESTING, PreparedScalar,
};
use crate::common::number_format::format_number;
use std::collections::{HashMap, HashSet};

pub const MAX_VECTOR_COMPONENTS: usize = 128;
const MAX_VECTOR_WORK: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct CalculationVector(Vec<f64>);

impl CalculationVector {
    fn new(values: Vec<f64>) -> Result<Self, DiagnosticKind> {
        if values.is_empty() || values.len() > MAX_VECTOR_COMPONENTS {
            return Err(error("vectors must contain 1 to 128 components"));
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(error("vector components must be finite"));
        }
        Ok(Self(values))
    }
    pub fn components(&self) -> &[f64] {
        &self.0
    }
    pub(crate) fn formatted_result(&self) -> Option<String> {
        let parts = self
            .0
            .iter()
            .map(|value| format_number(*value))
            .collect::<Option<Vec<_>>>()?;
        Some(format!("[{}]", parts.join(", ")))
    }
}

fn error(message: &str) -> DiagnosticKind {
    DiagnosticKind::Vector(message.to_owned())
}

#[derive(Debug)]
pub(super) struct VectorExpression {
    root: Node,
    references: Vec<String>,
}
#[derive(Debug)]
struct Node {
    kind: Kind,
    scalar: Option<PreparedScalar>,
}
#[derive(Debug)]
enum Kind {
    Scalar(PreparedScalar, Option<String>),
    Literal(Vec<Node>),
    Binary(Op, Box<Node>, Box<Node>),
    Product(Op, Box<Node>, Box<Node>),
}
#[derive(Debug, Clone, Copy)]
enum Op {
    Add,
    Subtract,
    Multiply,
    Divide,
    Dot,
    Cross,
    Unsupported,
}

pub(super) fn has_explicit_syntax(source: &str) -> bool {
    let mut chars = source.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch == '{' {
            for (_, c) in chars.by_ref() {
                if c == '}' {
                    break;
                }
            }
            continue;
        }
        if matches!(ch, '[' | ']' | '·' | '×') {
            return true;
        }
        if !ch.is_ascii_alphabetic() && ch != '_' {
            continue;
        }
        if source[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let mut end = start + ch.len_utf8();
        while let Some((_, c)) = chars.peek().copied() {
            if !c.is_alphanumeric() && c != '_' {
                break;
            }
            end += c.len_utf8();
            chars.next();
        }
        let name = &source[start..end];
        if (name.eq_ignore_ascii_case("dot") || name.eq_ignore_ascii_case("cross"))
            && source[end..].trim_start().starts_with('(')
        {
            return true;
        }
    }
    false
}

pub(super) fn prepare(
    source: &str,
    declared: &HashSet<String>,
) -> Result<VectorExpression, DiagnosticKind> {
    let mut budget = 0;
    let root = parse(source, declared, 0, &mut budget)?;
    let mut references = Vec::new();
    root.collect_references(&mut references);
    references.sort();
    references.dedup();
    Ok(VectorExpression { root, references })
}
impl VectorExpression {
    pub(super) fn references(&self) -> &[String] {
        &self.references
    }
    pub(super) fn evaluate(
        &self,
        values: &HashMap<String, CalculationValue>,
    ) -> Result<CalculationValue, DiagnosticKind> {
        let mut work = 0;
        self.root.evaluate(values, &mut work)
    }
}

fn bump(work: &mut usize, amount: usize) -> Result<(), DiagnosticKind> {
    *work = work
        .checked_add(amount)
        .ok_or_else(|| error("vector expression exceeds its work limit"))?;
    if *work > MAX_VECTOR_WORK {
        return Err(error("vector expression exceeds its work limit"));
    }
    Ok(())
}

fn parse(
    source: &str,
    declared: &HashSet<String>,
    depth: usize,
    work: &mut usize,
) -> Result<Node, DiagnosticKind> {
    bump(work, 1)?;
    if depth > MAX_EXPRESSION_NESTING {
        return Err(error("vector expression is nested too deeply"));
    }
    let source = source.trim();
    if source.is_empty() {
        return Err(error("missing vector operand"));
    }
    let (splits, enclosing) = scan_top(source)?;
    if enclosing {
        return parse(&source[1..source.len() - 1], declared, depth + 1, work);
    }
    // Select the last operator at the lowest priority for left associativity.
    if let Some((at, len, op, _)) = splits.into_iter().rev().min_by_key(|split| split.3) {
        let left = parse(&source[..at], declared, depth + 1, work)?;
        let right = parse(&source[at + len..], declared, depth + 1, work)?;
        let scalar = if !has_explicit_syntax(source) {
            document::prepare_vector_scalar(source, declared).ok()
        } else {
            None
        };
        return Ok(Node {
            kind: Kind::Binary(op, Box::new(left), Box::new(right)),
            scalar,
        });
    }
    if source.starts_with('[') {
        if !source.ends_with(']') {
            return Err(error("invalid vector literal"));
        }
        let inner = &source[1..source.len() - 1];
        let parts = split_arguments(inner)?;
        if parts.is_empty() || parts.len() > MAX_VECTOR_COMPONENTS {
            return Err(error("vectors must contain 1 to 128 components"));
        }
        bump(work, parts.len())?;
        let mut components = Vec::with_capacity(parts.len());
        for part in parts {
            if part.trim().is_empty() {
                return Err(error(
                    "vector components must be nonempty scalar expressions",
                ));
            }
            components.push(parse(part, declared, depth + 1, work)?);
        }
        return Ok(Node {
            kind: Kind::Literal(components),
            scalar: None,
        });
    }
    if let Some((name, body)) = product_call(source) {
        let arguments = split_arguments(body)?;
        if arguments.len() != 2 {
            return Err(error("dot and cross require exactly two vectors"));
        }
        let left = parse(arguments[0], declared, depth + 1, work)?;
        let right = parse(arguments[1], declared, depth + 1, work)?;
        let op = if name.eq_ignore_ascii_case("dot") {
            Op::Dot
        } else {
            Op::Cross
        };
        return Ok(Node {
            kind: Kind::Product(op, Box::new(left), Box::new(right)),
            scalar: None,
        });
    }
    let scalar = document::prepare_vector_scalar(source, declared)?;
    let bare = if scalar.var_names().len() == 1 {
        let name = &scalar.var_names()[0];
        if source == name
            || source.strip_prefix('{').and_then(|s| s.strip_suffix('}')) == Some(name.as_str())
        {
            Some(name.clone())
        } else {
            None
        }
    } else {
        None
    };
    Ok(Node {
        kind: Kind::Scalar(scalar, bare),
        scalar: None,
    })
}

fn product_call(source: &str) -> Option<(&str, &str)> {
    let open = source.find('(')?;
    let name = source[..open].trim();
    if !(name.eq_ignore_ascii_case("dot") || name.eq_ignore_ascii_case("cross"))
        || !source.ends_with(')')
    {
        return None;
    }
    Some((name, &source[open + 1..source.len() - 1]))
}

fn split_arguments(source: &str) -> Result<Vec<&str>, DiagnosticKind> {
    if source.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    let mut stack = Vec::new();
    let mut start = 0;
    let mut braces = false;
    for (index, ch) in source.char_indices() {
        if braces {
            if ch == '}' {
                braces = false;
            }
            continue;
        }
        match ch {
            '{' => braces = true,
            '(' | '[' => stack.push(ch),
            ')' => {
                if stack.pop() != Some('(') {
                    return Err(error("unbalanced vector expression"));
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return Err(error("unbalanced vector expression"));
                }
            }
            ',' if stack.is_empty() => {
                result.push(&source[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        if stack.len() > MAX_EXPRESSION_NESTING {
            return Err(error("vector expression is nested too deeply"));
        }
    }
    if braces || !stack.is_empty() {
        return Err(error("unbalanced vector expression"));
    }
    result.push(&source[start..]);
    Ok(result)
}

type Split = (usize, usize, Op, u8);
fn scan_top(source: &str) -> Result<(Vec<Split>, bool), DiagnosticKind> {
    let mut stack = Vec::new();
    let mut braces = false;
    let mut splits = Vec::new();
    let mut outer_end = None;
    for (at, ch) in source.char_indices() {
        if braces {
            if ch == '}' {
                braces = false;
            }
            continue;
        }
        match ch {
            '{' => {
                braces = true;
                continue;
            }
            '(' | '[' => {
                stack.push(ch);
                if stack.len() > MAX_EXPRESSION_NESTING {
                    return Err(error("vector expression is nested too deeply"));
                }
                continue;
            }
            ')' | ']' => {
                let expected = if ch == ')' { '(' } else { '[' };
                if stack.pop() != Some(expected) {
                    return Err(error("unbalanced vector expression"));
                }
                if stack.is_empty() && source.starts_with('(') && outer_end.is_none() {
                    outer_end = Some(at + 1);
                }
                continue;
            }
            _ => {}
        }
        if !stack.is_empty() {
            continue;
        }
        if ch.is_ascii_alphabetic()
            && source[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
        {
            let end = source[at..]
                .char_indices()
                .find(|(_, c)| !c.is_alphanumeric() && *c != '_')
                .map_or(source.len(), |(offset, _)| at + offset);
            let name = &source[at..end];
            let previous = source[..at].chars().rev().find(|c| !c.is_whitespace());
            let next = source[end..].chars().find(|c| !c.is_whitespace());
            if matches!(name, "min" | "max" | "atan2")
                && previous.is_some_and(|c| c.is_alphanumeric() || matches!(c, ')' | '}' | ']'))
                && next.is_some_and(|c| {
                    c.is_alphanumeric() || matches!(c, '(' | '{' | '[' | '+' | '-' | '.')
                })
            {
                splits.push((at, end - at, Op::Unsupported, 0));
            }
        }
        let (op, precedence) = match ch {
            '+' => (Op::Add, 0),
            '-' => (Op::Subtract, 0),
            '*' => (Op::Multiply, 1),
            '/' => (Op::Divide, 1),
            '%' => (Op::Unsupported, 1),
            '·' => (Op::Dot, 1),
            '×' => (Op::Cross, 1),
            _ => continue,
        };
        let previous = source[..at].chars().rev().find(|c| !c.is_whitespace());
        if matches!(ch, '+' | '-') {
            if previous
                .is_none_or(|c| matches!(c, '+' | '-' | '*' | '/' | '^' | '%' | ',' | '·' | '×'))
            {
                continue;
            }
            // A sign directly following a decimal exponent belongs to its number.
            let before = &source[..at];
            if before.ends_with(['e', 'E']) {
                let token = before
                    .trim_end_matches(['e', 'E'])
                    .rsplit(|c: char| {
                        c.is_whitespace() || matches!(c, '+' | '-' | '*' | '/' | '(' | ',')
                    })
                    .next()
                    .unwrap_or("");
                if !token.is_empty() && token.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    continue;
                }
            }
        }
        splits.push((at, ch.len_utf8(), op, precedence));
    }
    if braces || !stack.is_empty() {
        return Err(error("unbalanced vector expression"));
    }
    Ok((
        splits,
        source.starts_with('(') && outer_end == Some(source.len()),
    ))
}

impl Node {
    fn collect_references(&self, references: &mut Vec<String>) {
        match &self.kind {
            Kind::Scalar(scalar, _) => references.extend(scalar.var_names().iter().cloned()),
            Kind::Literal(components) => {
                for component in components {
                    component.collect_references(references);
                }
            }
            Kind::Binary(_, left, right) | Kind::Product(_, left, right) => {
                left.collect_references(references);
                right.collect_references(references);
            }
        }
    }
    fn evaluate(
        &self,
        values: &HashMap<String, CalculationValue>,
        work: &mut usize,
    ) -> Result<CalculationValue, DiagnosticKind> {
        bump(work, 1)?;
        if let Some(scalar) = &self.scalar
            && scalar
                .var_names()
                .iter()
                .all(|name| matches!(values.get(name), Some(CalculationValue::Scalar(_))))
        {
            return scalar_value(scalar, values).map(CalculationValue::Scalar);
        }
        match &self.kind {
            Kind::Scalar(scalar, bare) => {
                if let Some(CalculationValue::Vector(vector)) =
                    bare.as_ref().and_then(|name| values.get(name))
                {
                    return Ok(CalculationValue::Vector(vector.clone()));
                }
                scalar_value(scalar, values).map(CalculationValue::Scalar)
            }
            Kind::Literal(components) => {
                bump(work, components.len())?;
                let mut result = Vec::with_capacity(components.len());
                for component in components {
                    match component.evaluate(values, work)? {
                        CalculationValue::Scalar(value) => result.push(value),
                        _ => return Err(error("vector components must be dimensionless scalars")),
                    }
                }
                Ok(CalculationValue::Vector(CalculationVector::new(result)?))
            }
            Kind::Binary(op, left, right) | Kind::Product(op, left, right) => {
                let left = left.evaluate(values, work)?;
                let right = right.evaluate(values, work)?;
                apply(*op, left, right, work)
            }
        }
    }
}
fn scalar_value(
    scalar: &PreparedScalar,
    values: &HashMap<String, CalculationValue>,
) -> Result<f64, DiagnosticKind> {
    let mut args = Vec::with_capacity(scalar.var_names().len());
    for name in scalar.var_names() {
        match values.get(name) {
            Some(CalculationValue::Scalar(value)) => args.push(*value),
            Some(_) => {
                return Err(error(
                    "vector operations require dimensionless scalar or vector values",
                ));
            }
            None => return Err(DiagnosticKind::UnknownName(name.clone())),
        }
    }
    match scalar.eval(&args) {
        Ok(value) if value.is_finite() => Ok(value),
        Ok(_) => Err(DiagnosticKind::InvalidArithmetic),
        Err(_) => Err(DiagnosticKind::InvalidExpression),
    }
}
fn checked(value: f64) -> Result<f64, DiagnosticKind> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(DiagnosticKind::InvalidArithmetic)
    }
}
fn apply(
    op: Op,
    left: CalculationValue,
    right: CalculationValue,
    work: &mut usize,
) -> Result<CalculationValue, DiagnosticKind> {
    use CalculationValue::{Scalar, Vector};
    match (op, left, right) {
        (Op::Multiply, Scalar(a), Vector(b)) | (Op::Multiply, Vector(b), Scalar(a)) => {
            bump(work, b.0.len())?;
            Ok(Vector(CalculationVector::new(
                b.0.iter()
                    .map(|v| checked(v * a))
                    .collect::<Result<Vec<_>, _>>()?,
            )?))
        }
        (Op::Divide, Vector(a), Scalar(b)) => {
            if b == 0.0 {
                return Err(DiagnosticKind::InvalidArithmetic);
            }
            bump(work, a.0.len())?;
            Ok(Vector(CalculationVector::new(
                a.0.iter()
                    .map(|v| checked(v / b))
                    .collect::<Result<Vec<_>, _>>()?,
            )?))
        }
        (op, Vector(a), Vector(b)) => {
            if a.0.len() != b.0.len() {
                return Err(error("vector dimensions must match"));
            }
            bump(work, a.0.len())?;
            if matches!(op, Op::Cross) {
                if a.0.len() != 3 {
                    return Err(error("cross product requires three-component vectors"));
                }
                let mut result = Vec::with_capacity(3);
                for (i, j) in [(1, 2), (2, 0), (0, 1)] {
                    result.push(checked(
                        checked(a.0[i] * b.0[j])? - checked(a.0[j] * b.0[i])?,
                    )?);
                }
                return Ok(Vector(CalculationVector::new(result)?));
            }
            if matches!(op, Op::Dot) {
                let mut sum = 0.0;
                for (x, y) in a.0.iter().zip(&b.0) {
                    sum = checked(sum + checked(x * y)?)?;
                }
                return Ok(Scalar(sum));
            }
            let result =
                a.0.iter()
                    .zip(&b.0)
                    .map(|(x, y)| match op {
                        Op::Add => checked(x + y),
                        Op::Subtract => checked(x - y),
                        Op::Multiply => checked(x * y),
                        Op::Divide if *y != 0.0 => checked(x / y),
                        Op::Divide => Err(DiagnosticKind::InvalidArithmetic),
                        _ => Err(error("invalid vector operation")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
            Ok(Vector(CalculationVector::new(result)?))
        }
        (op, Scalar(a), Scalar(b)) => match op {
            Op::Add => checked(a + b),
            Op::Subtract => checked(a - b),
            Op::Multiply => checked(a * b),
            Op::Divide if b != 0.0 => checked(a / b),
            Op::Divide => Err(DiagnosticKind::InvalidArithmetic),
            _ => Err(error("dot and cross require vectors")),
        }
        .map(Scalar),
        _ => Err(error("unsupported vector and scalar operation")),
    }
}
