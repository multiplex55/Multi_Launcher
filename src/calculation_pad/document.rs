use crate::common::{
    number_format::format_number,
    numeric_base::{NumericBase, format_unsigned, parse_unsigned},
};
use crate::unit_conversion::{
    self, Approximation, ConversionError, ConversionRequest, QuantityComponent, Unit,
};
use exmex::{Express, FlatEx};
use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use super::operators::CheckedOperators;

pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024;
pub const MAX_SOURCE_LINES: usize = 256;
pub const MAX_EXPRESSION_BYTES: usize = 2048;
pub const MAX_EXPRESSION_NESTING: usize = 32;
pub const MAX_DEPENDENCIES: usize = 512;
pub const MAX_DEPENDENCIES_PER_LINE: usize = 64;
pub const MAX_NAME_BYTES: usize = 128;

type CheckedExpression = FlatEx<f64, CheckedOperators>;

/// A source-authoritative calculation document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalculationDocument {
    source: String,
}

impl CalculationDocument {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Replaces the complete editable source. Evaluation applies document bounds.
    pub fn replace_source(&mut self, source: impl Into<String>) {
        self.source = source.into();
    }

    /// Replaces one physical source line without changing the surrounding line endings.
    pub fn replace_line(&mut self, line_index: usize, replacement: &str) -> Result<(), EditError> {
        if replacement.contains('\n') || replacement.contains('\r') {
            return Err(EditError::ContainsLineBreak);
        }
        let range = line_content_range(&self.source, line_index)
            .ok_or(EditError::LineOutOfRange { line_index })?;
        self.source.replace_range(range, replacement);
        Ok(())
    }

    /// Reevaluates every source line from the current document contents.
    pub fn evaluate(&self) -> CalculationEvaluation<'_> {
        let source = self.source.as_str();
        if source.len() > MAX_DOCUMENT_BYTES {
            return CalculationEvaluation {
                source,
                lines: Vec::new(),
                diagnostic: Some(DocumentDiagnostic::DocumentTooLarge {
                    max_bytes: MAX_DOCUMENT_BYTES,
                }),
            };
        }

        let source_line_count = source.bytes().filter(|byte| *byte == b'\n').count() + 1;
        if source_line_count > MAX_SOURCE_LINES {
            return CalculationEvaluation {
                source,
                lines: Vec::new(),
                diagnostic: Some(DocumentDiagnostic::TooManyLines {
                    max_lines: MAX_SOURCE_LINES,
                }),
            };
        }

        let mut total_identifiers = 0;
        let mut prepared = Vec::with_capacity(source_line_count);
        let mut source_offset = 0;
        for raw_line in source.split('\n') {
            let raw_end = source_offset + raw_line.len();
            let content_end = if raw_line.ends_with('\r') {
                raw_end - 1
            } else {
                raw_end
            };
            let content = &source[source_offset..content_end];
            prepared.push(prepare_line(
                source_offset..content_end,
                content,
                &mut total_identifiers,
            ));
            source_offset = raw_end.saturating_add(1);
        }

        let definitions = assignment_definitions(&prepared);
        mark_duplicate_assignments(&mut prepared, &definitions);
        resolve_references(&mut prepared, &definitions);
        let cyclic_lines = cyclic_lines(&prepared);

        let mut outcomes = Vec::with_capacity(prepared.len());
        let mut values = vec![None; prepared.len()];
        let mut results = Vec::with_capacity(prepared.len());

        for (line_index, line) in prepared.iter().enumerate() {
            let outcome = evaluate_line(line_index, line, &cyclic_lines, &outcomes, &values);
            if let LineOutcome::Value(value) = &outcome {
                values[line_index] = Some(value.clone());
            }
            outcomes.push(outcome.clone());
            results.push(CalculationLineResult {
                line_index,
                source_range: line.source_range.clone(),
                assignment_name: line.assignment_name.clone(),
                dependency_indices: line.dependencies.clone(),
                outcome,
            });
        }

        CalculationEvaluation {
            source,
            lines: results,
            diagnostic: None,
        }
    }
}

fn line_content_range(source: &str, requested_line: usize) -> Option<Range<usize>> {
    let mut offset = 0;
    for (line_index, line) in source.split('\n').enumerate() {
        let end = offset + line.len();
        if line_index == requested_line {
            let content_end = if line.ends_with('\r') { end - 1 } else { end };
            return Some(offset..content_end);
        }
        offset = end.saturating_add(1);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    LineOutOfRange { line_index: usize },
    ContainsLineBreak,
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineOutOfRange { line_index } => {
                write!(formatter, "source line {} does not exist", line_index + 1)
            }
            Self::ContainsLineBreak => {
                formatter.write_str("a single-line edit cannot contain a line break")
            }
        }
    }
}

impl std::error::Error for EditError {}

/// Evaluation snapshot over the original, unchanged document source.
#[derive(Debug, Clone, PartialEq)]
pub struct CalculationEvaluation<'a> {
    pub source: &'a str,
    pub lines: Vec<CalculationLineResult>,
    pub diagnostic: Option<DocumentDiagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalculationLineResult {
    /// Zero-based physical source line index.
    pub line_index: usize,
    /// UTF-8 byte range for the line content, excluding its line ending.
    pub source_range: Range<usize>,
    pub assignment_name: Option<String>,
    /// Zero-based physical source line indexes this line depends on.
    pub dependency_indices: Vec<usize>,
    pub outcome: LineOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineOutcome {
    Blank,
    Comment,
    Value(CalculationValue),
    Error(Diagnostic),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalculationValue {
    Scalar(f64),
    PhysicalQuantity {
        value: f64,
        unit: Unit,
        approximations: Vec<Approximation>,
    },
    ExactBase {
        value: u128,
        base: NumericBase,
        digits: String,
    },
}

impl CalculationValue {
    /// Formats a result using the shared number formatter where it applies.
    pub fn formatted_result(&self) -> Option<String> {
        match self {
            Self::Scalar(value) => format_number(*value),
            Self::PhysicalQuantity { value, unit, .. } => {
                Some(format!("{} {}", format_number(*value)?, unit.symbol()))
            }
            Self::ExactBase { digits, .. } => Some(digits.clone()),
        }
    }

    pub fn is_approximate(&self) -> bool {
        matches!(
            self,
            Self::PhysicalQuantity {
                approximations,
                ..
            } if !approximations.is_empty()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentDiagnostic {
    DocumentTooLarge { max_bytes: usize },
    TooManyLines { max_lines: usize },
}

impl fmt::Display for DocumentDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DocumentTooLarge { max_bytes } => {
                write!(formatter, "document exceeds the {max_bytes}-byte limit")
            }
            Self::TooManyLines { max_lines } => {
                write!(formatter, "document exceeds the {max_lines}-line limit")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
}

impl Diagnostic {
    fn new(kind: DiagnosticKind) -> Self {
        Self { kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    InvalidAssignment,
    NameTooLong {
        max_bytes: usize,
    },
    ReservedName(String),
    DuplicateName(String),
    UnknownName(String),
    UnknownRowReference(String),
    ForwardReference {
        reference: String,
        dependency_line: usize,
    },
    SelfReference(String),
    CyclicDependency,
    DependencyFailed {
        dependency_line: usize,
    },
    ExpectedScalar(String),
    ExpectedPhysicalQuantity(String),
    ExpectedExactBase(String),
    InvalidExpression,
    InvalidArithmetic,
    InvalidBaseNumber,
    UnitConversion(ConversionError),
    ExpressionTooLong {
        max_bytes: usize,
    },
    NestingTooDeep {
        max_depth: usize,
    },
    TooManyIdentifiers {
        max_identifiers: usize,
    },
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        use DiagnosticKind as Kind;
        match &self.kind {
            Kind::InvalidAssignment => formatter.write_str("invalid assignment syntax"),
            Kind::NameTooLong { max_bytes } => {
                write!(formatter, "name exceeds the {max_bytes}-byte limit")
            }
            Kind::ReservedName(name) => write!(formatter, "{name} is a reserved name"),
            Kind::DuplicateName(name) => write!(formatter, "{name} is assigned more than once"),
            Kind::UnknownName(name) => write!(formatter, "unknown name: {name}"),
            Kind::UnknownRowReference(reference) => {
                write!(formatter, "no result is available for {reference}")
            }
            Kind::ForwardReference {
                reference,
                dependency_line,
            } => write!(
                formatter,
                "{reference} refers to later line {}",
                dependency_line + 1
            ),
            Kind::SelfReference(reference) => {
                write!(formatter, "{reference} cannot refer to its own line")
            }
            Kind::CyclicDependency => formatter.write_str("cyclic dependency"),
            Kind::DependencyFailed { dependency_line } => write!(
                formatter,
                "dependency on line {} did not produce a result",
                dependency_line + 1
            ),
            Kind::ExpectedScalar(reference) => {
                write!(formatter, "{reference} is not a scalar value")
            }
            Kind::ExpectedPhysicalQuantity(reference) => {
                write!(formatter, "{reference} is not a physical quantity")
            }
            Kind::ExpectedExactBase(reference) => {
                write!(formatter, "{reference} is not an exact base value")
            }
            Kind::InvalidExpression => formatter.write_str("invalid expression"),
            Kind::InvalidArithmetic => {
                formatter.write_str("division by zero or non-finite arithmetic result")
            }
            Kind::InvalidBaseNumber => formatter.write_str("invalid unsigned base value"),
            Kind::UnitConversion(error) => write!(formatter, "{error}"),
            Kind::ExpressionTooLong { max_bytes } => {
                write!(formatter, "expression exceeds the {max_bytes}-byte limit")
            }
            Kind::NestingTooDeep { max_depth } => {
                write!(formatter, "expression nesting exceeds {max_depth} levels")
            }
            Kind::TooManyIdentifiers { max_identifiers } => write!(
                formatter,
                "identifier count exceeds the {max_identifiers}-item limit"
            ),
        }
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Debug)]
struct PreparedLine {
    source_range: Range<usize>,
    assignment_name: Option<String>,
    statement: PreparedStatement,
    references: Vec<PreparedReference>,
    dependencies: Vec<usize>,
    diagnostic: Option<Diagnostic>,
}

#[derive(Debug)]
enum PreparedStatement {
    Blank,
    Comment,
    Scalar(Box<CheckedExpression>),
    PhysicalConversion(String),
    PhysicalReference {
        destination: Unit,
        destination_text: String,
    },
    BaseConversion {
        value: u128,
        base: NumericBase,
    },
    BaseReference {
        destination: NumericBase,
    },
    Invalid,
}

#[derive(Debug, Clone)]
struct PreparedReference {
    name: String,
    kind: ReferenceKind,
    target_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferenceKind {
    Named,
    Row,
    InvalidRow,
}

#[derive(Debug)]
struct AssignmentName {
    canonical: String,
    braced: bool,
}

fn prepare_line(
    source_range: Range<usize>,
    source_line: &str,
    total_identifiers: &mut usize,
) -> PreparedLine {
    let trimmed = source_line.trim();
    if trimmed.is_empty() {
        return PreparedLine::simple(source_range, PreparedStatement::Blank);
    }
    if trimmed.starts_with('#') {
        return PreparedLine::simple(source_range, PreparedStatement::Comment);
    }

    let (assignment_name, expression, assignment_diagnostic) =
        match find_assignment_delimiter(trimmed) {
            Some(index) => {
                let left = trimmed[..index].trim();
                let right = trimmed[index + 1..].trim();
                if find_assignment_delimiter(right).is_some() {
                    return PreparedLine::invalid(
                        source_range,
                        None,
                        DiagnosticKind::InvalidAssignment,
                    );
                }
                match parse_assignment_name(left) {
                    Ok(assignment) => {
                        let diagnostic = if is_row_reference_name(&assignment.canonical)
                            || (!assignment.braced && is_reserved_name(&assignment.canonical))
                        {
                            Some(Diagnostic::new(DiagnosticKind::ReservedName(
                                assignment.canonical.clone(),
                            )))
                        } else {
                            None
                        };
                        (Some(assignment.canonical), right, diagnostic)
                    }
                    Err(kind) => {
                        return PreparedLine::invalid(source_range, None, kind);
                    }
                }
            }
            None => (None, trimmed, None),
        };

    if let Some(diagnostic) = assignment_diagnostic {
        return PreparedLine {
            source_range,
            assignment_name,
            statement: PreparedStatement::Invalid,
            references: Vec::new(),
            dependencies: Vec::new(),
            diagnostic: Some(diagnostic),
        };
    }
    if expression.is_empty() {
        return PreparedLine::invalid(
            source_range,
            assignment_name,
            DiagnosticKind::InvalidExpression,
        );
    }
    if expression.len() > MAX_EXPRESSION_BYTES {
        return PreparedLine::invalid(
            source_range,
            assignment_name,
            DiagnosticKind::ExpressionTooLong {
                max_bytes: MAX_EXPRESSION_BYTES,
            },
        );
    }

    let identifier_count = match validate_expression_bounds(expression) {
        Ok(count) => count,
        Err(kind) => return PreparedLine::invalid(source_range, assignment_name, kind),
    };
    if total_identifiers.saturating_add(identifier_count) > MAX_DEPENDENCIES {
        return PreparedLine::invalid(
            source_range,
            assignment_name,
            DiagnosticKind::TooManyIdentifiers {
                max_identifiers: MAX_DEPENDENCIES,
            },
        );
    }
    *total_identifiers += identifier_count;

    if let Some(base_conversion) = parse_base_conversion(expression) {
        return match base_conversion {
            Ok((value, base)) => PreparedLine {
                source_range,
                assignment_name,
                statement: PreparedStatement::BaseConversion { value, base },
                references: Vec::new(),
                dependencies: Vec::new(),
                diagnostic: None,
            },
            Err(kind) => PreparedLine::invalid(source_range, assignment_name, kind),
        };
    }

    if let Some((source, destination)) = split_to_keyword(expression) {
        if let Some(reference) = parse_simple_reference(source) {
            if let Some(base) = NumericBase::from_alias(destination) {
                return PreparedLine {
                    source_range,
                    assignment_name,
                    statement: PreparedStatement::BaseReference { destination: base },
                    references: vec![reference],
                    dependencies: Vec::new(),
                    diagnostic: None,
                };
            }
            let Some(unit) = unit_conversion::unit_by_alias(destination) else {
                return PreparedLine::invalid(
                    source_range,
                    assignment_name,
                    DiagnosticKind::UnitConversion(ConversionError::UnknownUnit {
                        expression: destination.to_owned(),
                    }),
                );
            };
            return PreparedLine {
                source_range,
                assignment_name,
                statement: PreparedStatement::PhysicalReference {
                    destination: unit,
                    destination_text: destination.to_owned(),
                },
                references: vec![reference],
                dependencies: Vec::new(),
                diagnostic: None,
            };
        }
        return PreparedLine {
            source_range,
            assignment_name,
            statement: PreparedStatement::PhysicalConversion(expression.to_owned()),
            references: Vec::new(),
            dependencies: Vec::new(),
            diagnostic: None,
        };
    }

    match CheckedExpression::parse_wo_compile(expression) {
        Ok(parsed) => {
            let references = parsed
                .var_names()
                .iter()
                .map(|name| prepared_reference(name))
                .collect();
            PreparedLine {
                source_range,
                assignment_name,
                statement: PreparedStatement::Scalar(Box::new(parsed)),
                references,
                dependencies: Vec::new(),
                diagnostic: None,
            }
        }
        Err(_) => PreparedLine::invalid(
            source_range,
            assignment_name,
            DiagnosticKind::InvalidExpression,
        ),
    }
}

impl PreparedLine {
    fn simple(source_range: Range<usize>, statement: PreparedStatement) -> Self {
        Self {
            source_range,
            assignment_name: None,
            statement,
            references: Vec::new(),
            dependencies: Vec::new(),
            diagnostic: None,
        }
    }

    fn invalid(
        source_range: Range<usize>,
        assignment_name: Option<String>,
        kind: DiagnosticKind,
    ) -> Self {
        Self {
            source_range,
            assignment_name,
            statement: PreparedStatement::Invalid,
            references: Vec::new(),
            dependencies: Vec::new(),
            diagnostic: Some(Diagnostic::new(kind)),
        }
    }
}

fn find_assignment_delimiter(expression: &str) -> Option<usize> {
    let mut in_braces = false;
    for (index, character) in expression.char_indices() {
        match character {
            '{' => in_braces = true,
            '}' => in_braces = false,
            '=' if !in_braces => return Some(index),
            _ => {}
        }
    }
    None
}

fn parse_assignment_name(input: &str) -> Result<AssignmentName, DiagnosticKind> {
    let input = input.trim();
    if let Some(inner) = input
        .strip_prefix('{')
        .and_then(|name| name.strip_suffix('}'))
    {
        if inner.is_empty()
            || inner
                .chars()
                .any(|character| matches!(character, '{' | '}' | '\n' | '\r'))
        {
            return Err(DiagnosticKind::InvalidAssignment);
        }
        if inner.len() > MAX_NAME_BYTES {
            return Err(DiagnosticKind::NameTooLong {
                max_bytes: MAX_NAME_BYTES,
            });
        }
        return Ok(AssignmentName {
            canonical: inner.to_owned(),
            braced: true,
        });
    }

    if !is_bare_identifier(input) {
        return Err(DiagnosticKind::InvalidAssignment);
    }
    if input.len() > MAX_NAME_BYTES {
        return Err(DiagnosticKind::NameTooLong {
            max_bytes: MAX_NAME_BYTES,
        });
    }
    Ok(AssignmentName {
        canonical: input.to_owned(),
        braced: false,
    })
}

fn parse_simple_reference(input: &str) -> Option<PreparedReference> {
    let parsed = parse_assignment_name(input).ok()?;
    Some(prepared_reference(&parsed.canonical))
}

fn is_bare_identifier(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_identifier_start(first) || !chars.all(is_identifier_continue) {
        return false;
    }
    true
}

fn is_identifier_start(character: char) -> bool {
    character == '_'
        || character.is_ascii_alphabetic()
        || matches!(character, 'α'..='ω' | 'Α'..='Ω')
}

fn is_identifier_continue(character: char) -> bool {
    is_identifier_start(character) || character.is_ascii_digit()
}

fn is_reserved_name(name: &str) -> bool {
    matches!(
        name,
        "abs"
            | "signum"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "asinh"
            | "acosh"
            | "atanh"
            | "floor"
            | "round"
            | "ceil"
            | "trunc"
            | "fract"
            | "exp"
            | "sqrt"
            | "cbrt"
            | "ln"
            | "log2"
            | "log10"
            | "log"
            | "atan2"
            | "min"
            | "max"
            | "PI"
            | "π"
            | "E"
            | "e"
            | "TAU"
            | "τ"
    )
}

fn is_row_reference_name(name: &str) -> bool {
    name.strip_prefix('r').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn validate_expression_bounds(expression: &str) -> Result<usize, DiagnosticKind> {
    let mut identifiers = 0;
    let mut depth = 0;
    let mut characters = expression.char_indices().peekable();

    while let Some((_, character)) = characters.next() {
        match character {
            '{' => {
                let mut name_bytes = 0;
                loop {
                    match characters.next() {
                        Some((_, '}')) => break,
                        Some((_, '{' | '\n' | '\r')) | None => {
                            return Err(DiagnosticKind::InvalidExpression);
                        }
                        Some((_, character)) => name_bytes += character.len_utf8(),
                    }
                }
                if name_bytes == 0 {
                    return Err(DiagnosticKind::InvalidExpression);
                }
                if name_bytes > MAX_NAME_BYTES {
                    return Err(DiagnosticKind::NameTooLong {
                        max_bytes: MAX_NAME_BYTES,
                    });
                }
                identifiers += 1;
            }
            '}' => return Err(DiagnosticKind::InvalidExpression),
            '(' => {
                depth += 1;
                if depth > MAX_EXPRESSION_NESTING {
                    return Err(DiagnosticKind::NestingTooDeep {
                        max_depth: MAX_EXPRESSION_NESTING,
                    });
                }
            }
            ')' => {
                if depth == 0 {
                    return Err(DiagnosticKind::InvalidExpression);
                }
                depth -= 1;
            }
            character if is_identifier_start(character) => {
                let mut name_bytes = character.len_utf8();
                while let Some((_, next)) = characters.peek().copied() {
                    if !is_identifier_continue(next) {
                        break;
                    }
                    name_bytes += next.len_utf8();
                    characters.next();
                }
                if name_bytes > MAX_NAME_BYTES {
                    return Err(DiagnosticKind::NameTooLong {
                        max_bytes: MAX_NAME_BYTES,
                    });
                }
                identifiers += 1;
            }
            _ => {}
        }
        if identifiers > MAX_DEPENDENCIES_PER_LINE {
            return Err(DiagnosticKind::TooManyIdentifiers {
                max_identifiers: MAX_DEPENDENCIES_PER_LINE,
            });
        }
    }
    if depth != 0 {
        return Err(DiagnosticKind::InvalidExpression);
    }
    Ok(identifiers)
}

fn split_to_keyword(expression: &str) -> Option<(&str, &str)> {
    let mut in_braces = false;
    let mut found = None;
    for (index, character) in expression.char_indices() {
        match character {
            '{' => {
                in_braces = true;
                continue;
            }
            '}' => {
                in_braces = false;
                continue;
            }
            _ => {}
        }
        if in_braces || !character.eq_ignore_ascii_case(&'t') {
            continue;
        }
        let Some(separator) = expression.get(index..index.saturating_add(2)) else {
            continue;
        };
        if !separator.eq_ignore_ascii_case("to") {
            continue;
        }
        let before_is_boundary = expression[..index]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        let after_index = index + 2;
        let after_is_boundary = expression[after_index..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace);
        if !before_is_boundary || !after_is_boundary {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some((&expression[..index], &expression[after_index..]));
    }
    found.map(|(source, destination)| (source.trim(), destination.trim()))
}

fn parse_base_conversion(expression: &str) -> Option<Result<(u128, NumericBase), DiagnosticKind>> {
    let (source, destination) = split_to_keyword(expression)?;
    let destination = NumericBase::from_alias(destination)?;
    let mut fields = source.split_whitespace();
    let digits = fields.next()?;
    let from_alias = fields.next()?;
    let from = NumericBase::from_alias(from_alias)?;
    if fields.next().is_some() {
        return Some(Err(DiagnosticKind::InvalidBaseNumber));
    }
    Some(
        parse_unsigned(digits, from)
            .map(|value| (value, destination))
            .ok_or(DiagnosticKind::InvalidBaseNumber),
    )
}

fn prepared_reference(name: &str) -> PreparedReference {
    let (kind, target_line) = match parse_row_reference(name) {
        RowReference::NotRow => (ReferenceKind::Named, None),
        RowReference::Invalid => (ReferenceKind::InvalidRow, None),
        RowReference::Line(line_index) => (ReferenceKind::Row, Some(line_index)),
    };
    PreparedReference {
        name: name.to_owned(),
        kind,
        target_line,
    }
}

enum RowReference {
    NotRow,
    Invalid,
    Line(usize),
}

fn parse_row_reference(name: &str) -> RowReference {
    let Some(digits) = name.strip_prefix('r') else {
        return RowReference::NotRow;
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return RowReference::NotRow;
    }
    match digits.parse::<usize>() {
        Ok(0) | Err(_) => RowReference::Invalid,
        Ok(line) => RowReference::Line(line - 1),
    }
}

fn assignment_definitions(lines: &[PreparedLine]) -> HashMap<String, Vec<usize>> {
    let mut definitions = HashMap::<String, Vec<usize>>::new();
    for (line_index, line) in lines.iter().enumerate() {
        if let Some(name) = &line.assignment_name {
            definitions
                .entry(name.clone())
                .or_default()
                .push(line_index);
        }
    }
    definitions
}

fn mark_duplicate_assignments(
    lines: &mut [PreparedLine],
    definitions: &HashMap<String, Vec<usize>>,
) {
    for (name, locations) in definitions {
        if locations.len() < 2 {
            continue;
        }
        for &line_index in locations {
            set_diagnostic_if_missing(
                &mut lines[line_index],
                DiagnosticKind::DuplicateName(name.clone()),
            );
        }
    }
}

fn resolve_references(lines: &mut [PreparedLine], definitions: &HashMap<String, Vec<usize>>) {
    for line_index in 0..lines.len() {
        for reference_index in 0..lines[line_index].references.len() {
            let reference_name = lines[line_index].references[reference_index].name.clone();
            let target = match lines[line_index].references[reference_index].kind {
                ReferenceKind::Row => lines[line_index].references[reference_index]
                    .target_line
                    .filter(|target| *target < lines.len()),
                ReferenceKind::InvalidRow => {
                    set_diagnostic_if_missing(
                        &mut lines[line_index],
                        DiagnosticKind::UnknownRowReference(reference_name),
                    );
                    None
                }
                ReferenceKind::Named => match definitions.get(&reference_name) {
                    Some(locations) if locations.len() == 1 => Some(locations[0]),
                    Some(_) => {
                        set_diagnostic_if_missing(
                            &mut lines[line_index],
                            DiagnosticKind::DuplicateName(reference_name),
                        );
                        None
                    }
                    None => {
                        set_diagnostic_if_missing(
                            &mut lines[line_index],
                            DiagnosticKind::UnknownName(reference_name),
                        );
                        None
                    }
                },
            };

            if lines[line_index].references[reference_index].kind == ReferenceKind::Row
                && target.is_none()
            {
                let name = lines[line_index].references[reference_index].name.clone();
                set_diagnostic_if_missing(
                    &mut lines[line_index],
                    DiagnosticKind::UnknownRowReference(name),
                );
            }
            lines[line_index].references[reference_index].target_line = target;
            if let Some(target) = target {
                lines[line_index].dependencies.push(target);
            }
        }
        lines[line_index].dependencies.sort_unstable();
        lines[line_index].dependencies.dedup();
    }
}

fn set_diagnostic_if_missing(line: &mut PreparedLine, kind: DiagnosticKind) {
    if line.diagnostic.is_none() {
        line.diagnostic = Some(Diagnostic::new(kind));
    }
}

fn cyclic_lines(lines: &[PreparedLine]) -> Vec<bool> {
    fn visit(
        line_index: usize,
        lines: &[PreparedLine],
        state: &mut [u8],
        stack: &mut Vec<usize>,
        cyclic: &mut [bool],
    ) {
        state[line_index] = 1;
        stack.push(line_index);
        for &dependency in &lines[line_index].dependencies {
            match state[dependency] {
                0 => visit(dependency, lines, state, stack, cyclic),
                1 => {
                    if let Some(cycle_start) = stack.iter().position(|member| *member == dependency)
                    {
                        for &member in &stack[cycle_start..] {
                            cyclic[member] = true;
                        }
                    }
                }
                _ => {}
            }
        }
        stack.pop();
        state[line_index] = 2;
    }

    let mut state = vec![0; lines.len()];
    let mut stack = Vec::new();
    let mut cyclic = vec![false; lines.len()];
    for line_index in 0..lines.len() {
        if state[line_index] == 0 {
            visit(line_index, lines, &mut state, &mut stack, &mut cyclic);
        }
    }
    cyclic
}

fn evaluate_line(
    line_index: usize,
    line: &PreparedLine,
    cyclic: &[bool],
    outcomes: &[LineOutcome],
    values: &[Option<CalculationValue>],
) -> LineOutcome {
    if let Some(diagnostic) = &line.diagnostic {
        return LineOutcome::Error(diagnostic.clone());
    }

    match &line.statement {
        PreparedStatement::Blank => return LineOutcome::Blank,
        PreparedStatement::Comment => return LineOutcome::Comment,
        PreparedStatement::Invalid => {
            return LineOutcome::Error(Diagnostic::new(DiagnosticKind::InvalidExpression));
        }
        _ => {}
    }

    if let Some(reference) = line
        .references
        .iter()
        .find(|reference| reference.target_line == Some(line_index))
    {
        return LineOutcome::Error(Diagnostic::new(DiagnosticKind::SelfReference(
            reference.name.clone(),
        )));
    }
    if cyclic[line_index] {
        return LineOutcome::Error(Diagnostic::new(DiagnosticKind::CyclicDependency));
    }

    for reference in &line.references {
        let Some(dependency_line) = reference.target_line else {
            return LineOutcome::Error(Diagnostic::new(DiagnosticKind::UnknownName(
                reference.name.clone(),
            )));
        };
        if dependency_line > line_index {
            return LineOutcome::Error(Diagnostic::new(DiagnosticKind::ForwardReference {
                reference: reference.name.clone(),
                dependency_line,
            }));
        }
        if dependency_line == line_index {
            return LineOutcome::Error(Diagnostic::new(DiagnosticKind::SelfReference(
                reference.name.clone(),
            )));
        }
        match outcomes.get(dependency_line) {
            Some(LineOutcome::Value(_)) => {}
            Some(LineOutcome::Blank | LineOutcome::Comment) => {
                return LineOutcome::Error(Diagnostic::new(DiagnosticKind::UnknownRowReference(
                    reference.name.clone(),
                )));
            }
            Some(LineOutcome::Error(_)) | None => {
                return LineOutcome::Error(Diagnostic::new(DiagnosticKind::DependencyFailed {
                    dependency_line,
                }));
            }
        }
    }

    let result = match &line.statement {
        PreparedStatement::Blank | PreparedStatement::Comment | PreparedStatement::Invalid => {
            unreachable!("non-result line states returned above")
        }
        PreparedStatement::Scalar(expression) => {
            let mut variables = Vec::with_capacity(expression.var_names().len());
            for name in expression.var_names() {
                let Some(reference) = line
                    .references
                    .iter()
                    .find(|reference| reference.name == *name)
                else {
                    return LineOutcome::Error(Diagnostic::new(DiagnosticKind::UnknownName(
                        name.clone(),
                    )));
                };
                let Some(dependency_line) = reference.target_line else {
                    return LineOutcome::Error(Diagnostic::new(DiagnosticKind::UnknownName(
                        name.clone(),
                    )));
                };
                match values.get(dependency_line).and_then(Option::as_ref) {
                    Some(CalculationValue::Scalar(value)) => variables.push(*value),
                    Some(_) => {
                        return LineOutcome::Error(Diagnostic::new(
                            DiagnosticKind::ExpectedScalar(name.clone()),
                        ));
                    }
                    None => {
                        return LineOutcome::Error(Diagnostic::new(
                            DiagnosticKind::DependencyFailed { dependency_line },
                        ));
                    }
                }
            }
            match expression.eval(&variables) {
                Ok(value) if value.is_finite() => Ok(CalculationValue::Scalar(value)),
                Ok(_) => Err(DiagnosticKind::InvalidArithmetic),
                Err(_) => Err(DiagnosticKind::InvalidExpression),
            }
        }
        PreparedStatement::PhysicalConversion(expression) => {
            match unit_conversion::evaluate_conversion(expression) {
                Ok(outcome) => Ok(CalculationValue::PhysicalQuantity {
                    value: outcome.value,
                    unit: outcome.request.destination,
                    approximations: outcome.approximations,
                }),
                Err(error) => Err(DiagnosticKind::UnitConversion(error)),
            }
        }
        PreparedStatement::PhysicalReference {
            destination,
            destination_text,
        } => {
            let reference = &line.references[0];
            let dependency_line = reference.target_line.expect("references are resolved");
            let Some(CalculationValue::PhysicalQuantity {
                value,
                unit,
                approximations: prior_approximations,
            }) = values[dependency_line].as_ref()
            else {
                let kind = match values[dependency_line].as_ref() {
                    Some(_) => DiagnosticKind::ExpectedPhysicalQuantity(reference.name.clone()),
                    None => DiagnosticKind::DependencyFailed { dependency_line },
                };
                return LineOutcome::Error(Diagnostic::new(kind));
            };
            let request = ConversionRequest {
                source_expression: reference.name.clone(),
                destination_expression: destination_text.clone(),
                components: vec![QuantityComponent {
                    value: *value,
                    unit: *unit,
                }],
                destination: *destination,
            };
            match unit_conversion::evaluate_request(request) {
                Ok(outcome) => {
                    let mut approximations = prior_approximations.clone();
                    for approximation in outcome.approximations {
                        if !approximations.contains(&approximation) {
                            approximations.push(approximation);
                        }
                    }
                    Ok(CalculationValue::PhysicalQuantity {
                        value: outcome.value,
                        unit: outcome.request.destination,
                        approximations,
                    })
                }
                Err(error) => Err(DiagnosticKind::UnitConversion(error)),
            }
        }
        PreparedStatement::BaseConversion { value, base } => Ok(CalculationValue::ExactBase {
            value: *value,
            base: *base,
            digits: format_unsigned(*value, *base),
        }),
        PreparedStatement::BaseReference { destination } => {
            let reference = &line.references[0];
            let dependency_line = reference.target_line.expect("references are resolved");
            match values[dependency_line].as_ref() {
                Some(CalculationValue::ExactBase { value, .. }) => {
                    Ok(CalculationValue::ExactBase {
                        value: *value,
                        base: *destination,
                        digits: format_unsigned(*value, *destination),
                    })
                }
                Some(_) => Err(DiagnosticKind::ExpectedExactBase(reference.name.clone())),
                None => Err(DiagnosticKind::DependencyFailed { dependency_line }),
            }
        }
    };

    match result {
        Ok(value) => LineOutcome::Value(value),
        Err(kind) => LineOutcome::Error(Diagnostic::new(kind)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CalculationDocument, CalculationEvaluation, CalculationValue, DiagnosticKind, LineOutcome,
        MAX_DEPENDENCIES, MAX_DEPENDENCIES_PER_LINE, MAX_DOCUMENT_BYTES, MAX_EXPRESSION_BYTES,
        MAX_EXPRESSION_NESTING, MAX_NAME_BYTES, MAX_SOURCE_LINES,
    };
    use crate::common::numeric_base::NumericBase;
    use crate::unit_conversion::{Approximation, ConversionError, Unit};

    fn value<'a>(
        evaluation: &'a CalculationEvaluation<'_>,
        line_index: usize,
    ) -> &'a CalculationValue {
        match &evaluation.lines[line_index].outcome {
            LineOutcome::Value(value) => value,
            other => panic!("expected a value on line {line_index}, got {other:?}"),
        }
    }

    fn error<'a>(
        evaluation: &'a CalculationEvaluation<'_>,
        line_index: usize,
    ) -> &'a DiagnosticKind {
        match &evaluation.lines[line_index].outcome {
            LineOutcome::Error(diagnostic) => &diagnostic.kind,
            other => panic!("expected an error on line {line_index}, got {other:?}"),
        }
    }

    fn scalar(evaluation: &CalculationEvaluation<'_>, line_index: usize) -> f64 {
        match value(evaluation, line_index) {
            CalculationValue::Scalar(value) => *value,
            other => panic!("expected a scalar on line {line_index}, got {other:?}"),
        }
    }

    fn quantity<'a>(
        evaluation: &'a CalculationEvaluation<'_>,
        line_index: usize,
    ) -> (f64, Unit, &'a [Approximation]) {
        match value(evaluation, line_index) {
            CalculationValue::PhysicalQuantity {
                value,
                unit,
                approximations,
            } => (*value, *unit, approximations),
            other => panic!("expected a quantity on line {line_index}, got {other:?}"),
        }
    }

    #[test]
    fn preserves_source_lines_and_re_evaluates_edits_without_stale_results() {
        let mut document = CalculationDocument::new("# notes\r\nbase = 2\r\nbase * 3\r\n");
        let initial = document.evaluate();
        assert_eq!(initial.source, "# notes\r\nbase = 2\r\nbase * 3\r\n");
        assert_eq!(initial.lines.len(), 4);
        assert!(matches!(&initial.lines[0].outcome, LineOutcome::Comment));
        assert_eq!(initial.lines[1].assignment_name.as_deref(), Some("base"));
        assert_eq!(scalar(&initial, 1), 2.0);
        assert_eq!(scalar(&initial, 2), 6.0);
        assert_eq!(
            &initial.source[initial.lines[1].source_range.clone()],
            "base = 2"
        );
        drop(initial);

        document.replace_line(1, "base = 4").unwrap();
        let edited = document.evaluate();
        assert_eq!(edited.source, "# notes\r\nbase = 4\r\nbase * 3\r\n");
        assert_eq!(scalar(&edited, 2), 12.0);
        drop(edited);

        document.replace_line(1, "base = missing").unwrap();
        let invalid = document.evaluate();
        assert!(matches!(error(&invalid, 1), DiagnosticKind::UnknownName(_)));
        assert!(matches!(
            error(&invalid, 2),
            DiagnosticKind::DependencyFailed { dependency_line: 1 }
        ));
        assert!(document.replace_line(1, "a\nb").is_err());
        assert!(document.replace_line(99, "x").is_err());
    }

    #[test]
    fn resolves_named_and_one_based_physical_row_references() {
        let document = CalculationDocument::new("# inputs\nbase = 2\nbase + 3\nr2 * 4\n");
        let evaluation = document.evaluate();

        assert_eq!(scalar(&evaluation, 2), 5.0);
        assert_eq!(scalar(&evaluation, 3), 8.0);
        assert_eq!(evaluation.lines[3].dependency_indices, vec![1]);
        assert_eq!(evaluation.lines[3].source_range, 27..33);
        assert!(matches!(&evaluation.lines[4].outcome, LineOutcome::Blank));
    }

    #[test]
    fn diagnoses_forward_self_unknown_duplicate_reserved_and_failed_references() {
        let document = CalculationDocument::new(
            "later + 1\nlater = 2\nself = self + 1\nmissing + 1\nr99\nr0\nx = 1\nx = 2\nz = x + 1\nz + 1\nsin = 4\n",
        );
        let evaluation = document.evaluate();

        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::ForwardReference {
                dependency_line: 1,
                ..
            }
        ));
        assert!(matches!(
            error(&evaluation, 2),
            DiagnosticKind::SelfReference(_)
        ));
        assert!(matches!(
            error(&evaluation, 3),
            DiagnosticKind::UnknownName(_)
        ));
        assert!(matches!(
            error(&evaluation, 4),
            DiagnosticKind::UnknownRowReference(_)
        ));
        assert!(matches!(
            error(&evaluation, 5),
            DiagnosticKind::UnknownRowReference(_)
        ));
        assert!(matches!(
            error(&evaluation, 6),
            DiagnosticKind::DuplicateName(_)
        ));
        assert!(matches!(
            error(&evaluation, 7),
            DiagnosticKind::DuplicateName(_)
        ));
        assert!(matches!(
            error(&evaluation, 8),
            DiagnosticKind::DuplicateName(_)
        ));
        assert!(matches!(
            error(&evaluation, 9),
            DiagnosticKind::DependencyFailed { dependency_line: 8 }
        ));
        assert!(matches!(
            error(&evaluation, 10),
            DiagnosticKind::ReservedName(_)
        ));

        let cycle_document = CalculationDocument::new("a = b\nb = a\nc = a\n");
        let cycle = cycle_document.evaluate();
        assert!(matches!(error(&cycle, 0), DiagnosticKind::CyclicDependency));
        assert!(matches!(error(&cycle, 1), DiagnosticKind::CyclicDependency));
        assert!(matches!(
            error(&cycle, 2),
            DiagnosticKind::DependencyFailed { dependency_line: 0 }
        ));
    }

    #[test]
    fn supports_greek_bare_and_braced_unicode_names() {
        let document = CalculationDocument::new(
            "α = 2\nα + 1\n{顧客 名} = 4\n{顧客 名} * 2\n{sin} = 5\n{sin} + 1\n",
        );
        let evaluation = document.evaluate();

        assert_eq!(scalar(&evaluation, 1), 3.0);
        assert_eq!(
            evaluation.lines[2].assignment_name.as_deref(),
            Some("顧客 名")
        );
        assert_eq!(scalar(&evaluation, 3), 8.0);
        assert_eq!(scalar(&evaluation, 5), 6.0);
    }

    #[test]
    fn rejects_bare_assignments_that_would_shadow_builtins_or_row_references() {
        let reserved_names = ["PI", "π", "E", "e", "TAU", "τ", "sin", "min", "r1"];
        let source = reserved_names
            .iter()
            .map(|name| format!("{name} = 2"))
            .collect::<Vec<_>>()
            .join("\n");
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();

        for line_index in 0..reserved_names.len() {
            assert!(matches!(
                error(&evaluation, line_index),
                DiagnosticKind::ReservedName(_)
            ));
        }
    }

    #[test]
    fn rejects_non_finite_intermediates_even_when_an_outer_operation_masks_them() {
        let document = CalculationDocument::new(
            "1 / (1 / 0)\n(1 / 0) ^ 0\nmin(sqrt(-1), 1)\nsignum(1 / 0)\n3\n",
        );
        let evaluation = document.evaluate();

        for line_index in 0..4 {
            assert!(matches!(
                error(&evaluation, line_index),
                DiagnosticKind::InvalidArithmetic
            ));
        }
        assert_eq!(scalar(&evaluation, 4), 3.0);
    }

    #[test]
    fn shares_catalog_quantity_conversion_and_keeps_quantity_types_separate() {
        let document = CalculationDocument::new(
            "1 km to m\nr1 to cm\n6 ft 2 in to cm\n1 month to day\n1 MB to Mb\n1 m to kg\nq = 1 m to cm\nq + 1\n1 km + 1\n7\nr10 to cm\n",
        );
        let evaluation = document.evaluate();

        let (value, unit, _) = quantity(&evaluation, 0);
        assert_eq!(value, 1000.0);
        assert_eq!(unit, Unit::Meter);
        let (value, unit, _) = quantity(&evaluation, 1);
        assert_eq!(value, 100_000.0);
        assert_eq!(unit, Unit::Centimeter);
        let (value, unit, _) = quantity(&evaluation, 2);
        assert!((value - 187.96).abs() < 1e-10);
        assert_eq!(unit, Unit::Centimeter);
        let (value, _, approximations) = quantity(&evaluation, 3);
        assert_eq!(value, 30.0);
        assert_eq!(approximations, &[Approximation::ThirtyDayMonth]);
        let (value, _, _) = quantity(&evaluation, 4);
        assert_eq!(value, 8.0);
        assert!(matches!(
            error(&evaluation, 5),
            DiagnosticKind::UnitConversion(ConversionError::IncompatibleUnits { .. })
        ));
        assert!(matches!(
            error(&evaluation, 7),
            DiagnosticKind::ExpectedScalar(_)
        ));
        // Scalar expressions do not imply multiplication between a value and a unit.
        assert!(matches!(
            error(&evaluation, 8),
            DiagnosticKind::InvalidExpression
        ));
        assert!(matches!(
            error(&evaluation, 10),
            DiagnosticKind::ExpectedPhysicalQuantity(_)
        ));
    }

    #[test]
    fn named_quantity_conversions_retain_prior_approximation_metadata() {
        let document = CalculationDocument::new("duration = 1 month to day\nduration to hour\n");
        let evaluation = document.evaluate();

        let (converted_value, unit, approximations) = quantity(&evaluation, 0);
        assert_eq!(converted_value, 30.0);
        assert_eq!(unit, Unit::Day);
        assert_eq!(approximations, &[Approximation::ThirtyDayMonth]);
        let (converted_value, unit, approximations) = quantity(&evaluation, 1);
        assert_eq!(converted_value, 720.0);
        assert_eq!(unit, Unit::Hour);
        assert_eq!(approximations, &[Approximation::ThirtyDayMonth]);
        assert!(value(&evaluation, 1).is_approximate());
    }

    #[test]
    fn converts_exact_bases_and_preserves_values_past_f64_precision() {
        let document = CalculationDocument::new(
            "9007199254740993 dec to hex\nr1 to bin\n340282366920938463463374607431768211455 dec to hex\n2 + 3\nr4 to oct\n",
        );
        let evaluation = document.evaluate();

        assert!(matches!(
            value(&evaluation, 0),
            CalculationValue::ExactBase {
                value: 9_007_199_254_740_993,
                base: NumericBase::Hexadecimal,
                digits,
            } if digits == "20000000000001"
        ));
        assert!(matches!(
            value(&evaluation, 1),
            CalculationValue::ExactBase {
                value: 9_007_199_254_740_993,
                base: NumericBase::Binary,
                digits,
            } if digits == "100000000000000000000000000000000000000000000000000001"
        ));
        assert!(matches!(
            value(&evaluation, 2),
            CalculationValue::ExactBase {
                value: u128::MAX,
                base: NumericBase::Hexadecimal,
                ..
            }
        ));
        assert!(matches!(
            error(&evaluation, 4),
            DiagnosticKind::ExpectedExactBase(_)
        ));
        assert_eq!(
            value(&evaluation, 0).formatted_result().as_deref(),
            Some("20000000000001")
        );
    }

    #[test]
    fn enforces_document_expression_nesting_name_and_dependency_bounds() {
        let document = CalculationDocument::new("x".repeat(MAX_DOCUMENT_BYTES + 1));
        assert!(matches!(
            document.evaluate().diagnostic,
            Some(super::DocumentDiagnostic::DocumentTooLarge { .. })
        ));

        let document = CalculationDocument::new("\n".repeat(MAX_SOURCE_LINES));
        assert!(matches!(
            document.evaluate().diagnostic,
            Some(super::DocumentDiagnostic::TooManyLines { .. })
        ));

        let too_long_expression = format!("1 + {}", "1".repeat(MAX_EXPRESSION_BYTES));
        let document = CalculationDocument::new(too_long_expression);
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::ExpressionTooLong { .. }
        ));

        let too_deep = format!(
            "{}1{}",
            "(".repeat(MAX_EXPRESSION_NESTING + 1),
            ")".repeat(MAX_EXPRESSION_NESTING + 1)
        );
        let document = CalculationDocument::new(too_deep);
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::NestingTooDeep { .. }
        ));

        let too_many_names = format!(
            "{}",
            std::iter::repeat_n("name", MAX_DEPENDENCIES_PER_LINE + 1)
                .collect::<Vec<_>>()
                .join(" + ")
        );
        let document = CalculationDocument::new(too_many_names);
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::TooManyIdentifiers { .. }
        ));

        let many_dependencies = std::iter::repeat_n(
            std::iter::repeat_n("x", MAX_DEPENDENCIES_PER_LINE)
                .collect::<Vec<_>>()
                .join(" + "),
            MAX_DEPENDENCIES / MAX_DEPENDENCIES_PER_LINE + 1,
        )
        .collect::<Vec<_>>()
        .join("\n");
        let document = CalculationDocument::new(many_dependencies);
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, MAX_DEPENDENCIES / MAX_DEPENDENCIES_PER_LINE),
            DiagnosticKind::TooManyIdentifiers { .. }
        ));

        let too_long_assignment = format!("{{{}}} = 2", "n".repeat(MAX_NAME_BYTES + 1));
        let document = CalculationDocument::new(too_long_assignment);
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::NameTooLong { .. }
        ));

        let too_long_reference = format!("{} + 1", "n".repeat(MAX_NAME_BYTES + 1));
        let document = CalculationDocument::new(too_long_reference);
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::NameTooLong { .. }
        ));
    }

    #[test]
    fn repeated_evaluation_is_deterministic_and_does_not_reuse_prior_values() {
        let mut document = CalculationDocument::new("count = 4\ncount / 2\n");
        let first = document.evaluate();
        let second = document.evaluate();
        assert_eq!(first, second);
        drop((first, second));

        document.replace_source("count = 1 / 0\ncount + 1\nindependent = 9\n");
        let updated = document.evaluate();
        assert!(matches!(
            error(&updated, 0),
            DiagnosticKind::InvalidArithmetic
        ));
        assert!(matches!(
            error(&updated, 1),
            DiagnosticKind::DependencyFailed { dependency_line: 0 }
        ));
        assert_eq!(scalar(&updated, 2), 9.0);
    }
}
