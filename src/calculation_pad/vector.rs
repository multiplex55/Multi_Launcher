//! Bounded vector expressions composed with the ordinary scalar evaluator.
use super::document::{
    self, CalculationValue, DiagnosticKind, MAX_EXPRESSION_BYTES, MAX_EXPRESSION_NESTING,
    PreparedScalar, script::WorkBudget,
};
use crate::common::number_format::format_number;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const MAX_VECTOR_COMPONENTS: usize = 1024;
pub(crate) const MAX_VECTOR_PARSE_WORK: usize = 8192;

#[derive(Debug, Clone, PartialEq)]
pub struct CalculationVector(Arc<Vec<f64>>);

impl CalculationVector {
    fn new(values: Vec<f64>) -> Result<Self, DiagnosticKind> {
        check_component_count(values.len())?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err(error("vector components must be finite"));
        }
        Ok(Self(Arc::new(values)))
    }
    pub fn components(&self) -> &[f64] {
        &self.0
    }
    pub(crate) fn formatted_result(&self) -> Option<String> {
        const PREVIEW_COMPONENTS: usize = 8;
        let parts = self
            .0
            .iter()
            .take(PREVIEW_COMPONENTS)
            .map(|value| format_number(*value))
            .collect::<Option<Vec<_>>>()?;
        if self.0.len() <= PREVIEW_COMPONENTS {
            Some(format!("[{}]", parts.join(", ")))
        } else {
            Some(format!("[{}, …] ({} values)", parts.join(", "), self.0.len()))
        }
    }

    pub(crate) fn canonical_result(&self) -> String {
        let parts = self.0.iter().map(|value| {
            // Reuse the readable display token only when it preserves every
            // bit and remains within the ordinary exact-integer admission.
            if let Some(text) = format_number(*value)
                && text.parse::<f64>().is_ok_and(|parsed| parsed.to_bits() == value.to_bits())
                && (text.contains(['.', 'e', 'E']) || value.abs() <= (1_u64 << 53) as f64)
            {
                text
            } else {
                format!("{value:e}")
            }
        }).collect::<Vec<_>>();
        format!("[{}]", parts.join(", "))
    }
}

/// Collection operations construct new vectors and never alter an input snapshot.
#[derive(Debug, Clone, Copy)]
pub(super) enum CollectionOperation {
    Append, Concat, Insert, SetAt, RemoveAt, Pop, Get, First, Last, Len, Slice, Reverse,
}

impl CollectionOperation {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "append" => Self::Append, "concat" => Self::Concat, "insert" => Self::Insert,
            "set_at" => Self::SetAt, "remove_at" => Self::RemoveAt, "pop" => Self::Pop,
            "get" => Self::Get, "first" => Self::First, "last" => Self::Last,
            "len" => Self::Len, "slice" => Self::Slice, "reverse" => Self::Reverse,
            _ => return None,
        })
    }

    pub(super) fn call(self, arguments: &[CalculationValue], work: &mut WorkBudget) -> Result<CalculationValue, DiagnosticKind> {
        use CalculationValue::{Scalar, Vector};
        let Some(Vector(vector)) = arguments.first() else {
            return Err(error("collection operation requires a vector"));
        };
        let values = vector.components();
        let scalar = |value: &CalculationValue| match value {
            Scalar(value) if value.is_finite() => Ok(*value),
            _ => Err(error("vector element must be a finite dimensionless scalar")),
        };
        let index = |value: &CalculationValue, limit: usize, inclusive: bool| {
            let Scalar(value) = value else { return Err(error("index must be a nonnegative integral scalar")); };
            if !value.is_finite() || *value < 0.0 || value.fract() != 0.0
                || if inclusive { *value > limit as f64 } else { *value >= limit as f64 }
            {
                return Err(error("index is invalid or out of bounds"));
            }
            Ok(*value as usize)
        };
        let result = match (self, arguments) {
            (Self::Len, [_]) => return Ok(Scalar(values.len() as f64)),
            (Self::Get, [_, position]) => {
                let position = index(position, values.len(), false)?;
                return values.get(position).copied().map(Scalar).ok_or_else(|| error("index is out of bounds"));
            }
            (Self::First, [_]) => return values.first().copied().map(Scalar).ok_or_else(|| error("vector is empty")),
            (Self::Last, [_]) => return values.last().copied().map(Scalar).ok_or_else(|| error("vector is empty")),
            (Self::Append, [_, value]) => {
                let value = scalar(value)?;
                check_component_count(values.len() + 1)?;
                work.charge_vector(values.len() + 1)?;
                let mut result = Vec::with_capacity(values.len() + 1);
                result.extend_from_slice(values);
                result.push(value);
                result
            }
            (Self::Concat, [_, Vector(other)]) => {
                let length = values.len() + other.components().len();
                check_component_count(length)?;
                work.charge_vector(length)?;
                let mut result = Vec::with_capacity(length);
                result.extend_from_slice(values);
                result.extend_from_slice(other.components());
                result
            }
            (Self::Insert, [_, position, value]) | (Self::SetAt, [_, position, value]) => {
                let insert = matches!(self, Self::Insert);
                let position = index(position, values.len(), insert)?;
                let value = scalar(value)?;
                let length = values.len() + usize::from(insert);
                check_component_count(length)?;
                work.charge_vector(length)?;
                let mut result = Vec::with_capacity(length);
                result.extend_from_slice(&values[..position]);
                result.push(value);
                result.extend_from_slice(&values[position + usize::from(!insert)..]);
                result
            }
            (Self::RemoveAt, [_, position]) => {
                let position = index(position, values.len(), false)?;
                work.charge_vector(values.len() - 1)?;
                let mut result = Vec::with_capacity(values.len() - 1);
                result.extend_from_slice(&values[..position]);
                result.extend_from_slice(&values[position + 1..]);
                result
            }
            (Self::Pop, [_]) => {
                if values.is_empty() { return Err(error("vector is empty")); }
                work.charge_vector(values.len() - 1)?;
                values[..values.len() - 1].to_vec()
            }
            (Self::Slice, [_, start, end]) => {
                let start = index(start, values.len(), true)?;
                let end = index(end, values.len(), true)?;
                if start > end { return Err(error("slice start must not exceed end")); }
                work.charge_vector(end - start)?;
                values[start..end].to_vec()
            }
            (Self::Reverse, [_]) => {
                work.charge_vector(values.len())?;
                values.iter().rev().copied().collect()
            }
            _ => return Err(error("invalid collection operation arguments")),
        };
        // Inputs and inserted scalars were already validated, so no second
        // component scan is needed after these pure copying transformations.
        Ok(Vector(CalculationVector(Arc::new(result))))
    }
}

fn error(message: &str) -> DiagnosticKind {
    DiagnosticKind::Vector(message.to_owned())
}

fn check_component_count(count: usize) -> Result<(), DiagnosticKind> {
    if count > MAX_VECTOR_COMPONENTS {
        return Err(error("vectors may contain at most 1024 components"));
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct VectorExpression {
    root: Node,
    references: Vec<String>,
    source_bytes: usize,
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
    Ok(VectorExpression { root, references, source_bytes: source.len() })
}
impl VectorExpression {
    pub(super) fn references(&self) -> &[String] {
        &self.references
    }
    pub(super) fn evaluate(
        &self,
        values: &HashMap<String, CalculationValue>,
        work: &mut WorkBudget,
    ) -> Result<CalculationValue, DiagnosticKind> {
        work.charge_vector(self.source_bytes)?;
        self.root.evaluate(values, work)
    }
}

fn bump(work: &mut usize, amount: usize) -> Result<(), DiagnosticKind> {
    *work = work
        .checked_add(amount)
        .ok_or_else(|| error("vector expression exceeds its work limit"))?;
    if *work > MAX_VECTOR_PARSE_WORK {
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
        let parts = split_arguments(inner, true)?;
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
        let arguments = split_arguments(body, false)?;
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
    if source.len() > MAX_EXPRESSION_BYTES {
        return Err(DiagnosticKind::ExpressionTooLong { max_bytes: MAX_EXPRESSION_BYTES });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn vector_work_uses_aggregate_budget_and_cancellation() {
        let expression = prepare("[1, 2, 3]", &HashSet::new()).unwrap();
        let aggregate = Arc::new(document::script::AggregateWork::new(2));
        let canceled = Arc::new(AtomicBool::new(false));
        let mut work = WorkBudget::with_analysis(canceled.clone(), aggregate.clone());
        assert!(matches!(expression.evaluate(&HashMap::new(), &mut work), Err(DiagnosticKind::WorkLimit)));
        assert!(aggregate.exhausted());
        canceled.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut work = WorkBudget::with_cancellation(canceled);
        assert!(matches!(expression.evaluate(&HashMap::new(), &mut work), Err(DiagnosticKind::Canceled)));
    }

    #[test]
    fn vector_component_budget_is_bounded_and_weighted_for_analysis() {
        let aggregate = Arc::new(document::script::AggregateWork::new(2));
        let mut work = WorkBudget::with_analysis(Arc::new(AtomicBool::new(false)), aggregate.clone());
        assert!(work.charge_vector(32).is_ok());
        assert!(work.charge_vector(32).is_ok());
        assert!(matches!(work.charge_vector(1), Err(DiagnosticKind::WorkLimit)));
        assert!(aggregate.exhausted());
        let mut work = WorkBudget::default();
        assert!(work.charge_vector(document::script::MAX_VECTOR_COMPONENT_WORK).is_ok());
        assert!(matches!(work.charge_vector(1), Err(DiagnosticKind::WorkLimit)));
    }

    #[test]
    fn vector_preview_is_bounded_and_canonical_copy_round_trips_all_bits() {
        let sample = [
            0.0, -0.0, f64::from_bits(1), -f64::from_bits(1), f64::MIN_POSITIVE,
            f64::MAX, -f64::MAX, 1.0 / 3.0, 9_007_199_254_740_994.0, -1.234_567_890_123_456_7e-200,
        ];
        let input = (0..MAX_VECTOR_COMPONENTS).map(|index| sample[index % sample.len()]).collect::<Vec<_>>();
        let vector = CalculationVector::new(input.clone()).unwrap();
        let preview = vector.formatted_result().unwrap();
        assert!(preview.contains('…'));
        assert!(preview.contains("1024 values"));
        assert!(preview.len() < 200);
        let canonical = CalculationValue::Vector(vector).canonical_result().unwrap();
        assert!(!canonical.contains('…'));
        assert!(!canonical.contains("values"));
        assert!(canonical.len() <= document::MAX_VECTOR_EXPRESSION_BYTES);
        let document = document::CalculationDocument::new(canonical);
        let evaluation = document.evaluate();
        let document::LineOutcome::Value(CalculationValue::Vector(output)) = &evaluation.lines[0].outcome else {
            panic!("canonical vector did not evaluate: {:?}", evaluation.lines[0].outcome);
        };
        assert_eq!(output.components().len(), input.len());
        for (index, (actual, expected)) in output.components().iter().zip(&input).enumerate() {
            assert_eq!(actual.to_bits(), expected.to_bits(), "component {index}");
        }
        let small = CalculationVector::new(vec![1.0, 2.0, 3.0]).unwrap();
        assert_eq!(small.formatted_result().as_deref(), Some("[1, 2, 3]"));
        assert_eq!(small.canonical_result(), "[1, 2, 3]");
    }
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

fn split_arguments(source: &str, vector_literal: bool) -> Result<Vec<&str>, DiagnosticKind> {
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
                if vector_literal { check_component_count(result.len() + 1)?; }
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
        work: &mut WorkBudget,
    ) -> Result<CalculationValue, DiagnosticKind> {
        work.charge(1)?;
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
                work.charge_vector(components.len().saturating_mul(2))?;
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
    work: &mut WorkBudget,
) -> Result<CalculationValue, DiagnosticKind> {
    use CalculationValue::{Scalar, Vector};
    match (op, left, right) {
        (Op::Multiply, Scalar(a), Vector(b)) | (Op::Multiply, Vector(b), Scalar(a)) => {
            work.charge_vector(b.0.len().saturating_mul(2))?;
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
            work.charge_vector(a.0.len().saturating_mul(2))?;
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
            if matches!(op, Op::Unsupported) {
                return Err(error("invalid vector operation"));
            }
            work.charge_vector(a.0.len().saturating_mul(2))?;
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
                for (x, y) in a.0.iter().zip(b.0.iter()) {
                    sum = checked(sum + checked(x * y)?)?;
                }
                return Ok(Scalar(sum));
            }
            let result =
                a.0.iter()
                    .zip(b.0.iter())
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
