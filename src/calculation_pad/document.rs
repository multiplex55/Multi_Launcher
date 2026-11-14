use crate::common::{
    number_format::format_number,
    numeric_base::{NumericBase, format_unsigned, parse_unsigned},
};
use crate::unit_conversion::{
    self, Approximation, ConversionError, ConversionRequest, QuantityComponent, Unit,
};
use exmex::{Express, FlatEx};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Range;

use super::integer::TypedInteger;
use super::operators::{CheckedOperators, ModuloOperators};
use super::programmer::{self, ProgrammerError, ProgrammerExpression};
use super::quantity::{
    self, CalculatedQuantity, QuantityError, QuantityExpression, UnitParseEvidence, UnitProduct,
};
use super::vector::{self, VectorExpression};
#[path = "script.rs"]
pub(super) mod script;
pub use script::CalculationSequence;

pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024;
pub const MAX_SOURCE_LINES: usize = 256;
pub const MAX_EXPRESSION_BYTES: usize = 2048;
pub const MAX_EXPRESSION_NESTING: usize = 32;
pub const MAX_DEPENDENCIES: usize = 512;
pub const MAX_DEPENDENCIES_PER_LINE: usize = 64;
pub const MAX_NAME_BYTES: usize = 128;
const MAX_PREPARED_EXPRESSION_BYTES: usize = MAX_EXPRESSION_BYTES * 4;

type CheckedExpression = FlatEx<f64, CheckedOperators>;
type ModuloExpression = FlatEx<f64, ModuloOperators>;

const MAX_EXACT_SCALAR_INTEGER: u64 = 1_u64 << 53;

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
                final_bindings: HashMap::new(),
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
                final_bindings: HashMap::new(),
                diagnostic: Some(DocumentDiagnostic::TooManyLines {
                    max_lines: MAX_SOURCE_LINES,
                }),
            };
        }

        // Alias fallback must see forward declarations too, so
        // references keep the same diagnostics as ordinary named variables.
        let declared_names = declared_names(source);
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
                &declared_names,
            ));
            source_offset = raw_end.saturating_add(1);
        }

        let definitions = assignment_definitions(&prepared);
        resolve_references(&mut prepared, &definitions);
        let cyclic_lines = cyclic_lines(&prepared);

        let mut outcomes = Vec::with_capacity(prepared.len());
        let mut values = vec![None; prepared.len()];
        let mut results = Vec::with_capacity(prepared.len());

        let mut work = script::WorkBudget::default();
        for (line_index, line) in prepared.iter().enumerate() {
            let mut dependencies = line.dependencies.clone();
            let outcome = if let PreparedStatement::Script(expression) = &line.statement {
                let result = expression.evaluate(line_index, &outcomes, &values, &mut dependencies, &mut work);
                match result { Ok(value) => LineOutcome::Value(value), Err(kind) => LineOutcome::Error(Diagnostic::new(kind)) }
            } else {
                evaluate_line(line_index, line, &cyclic_lines, &outcomes, &values)
            };
            dependencies.sort_unstable();
            dependencies.dedup();
            if let LineOutcome::Value(value) = &outcome {
                values[line_index] = Some(value.clone());
            }
            outcomes.push(outcome.clone());
            results.push(CalculationLineResult {
                line_index,
                source_range: line.source_range.clone(),
                assignment_name: line.assignment_name.clone(),
                dependency_indices: dependencies,
                outcome,
            });
        }

        CalculationEvaluation {
            source,
            lines: results,
            final_bindings: definitions
                .into_iter()
                .filter_map(|(name, lines)| lines.last().copied().map(|line| (name, line)))
                .collect(),
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
    /// The physical row of each name's latest binding, including failures.
    pub(crate) final_bindings: HashMap<String, usize>,
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
    Boolean(bool),
    Sequence(CalculationSequence),
    Vector(vector::CalculationVector),
    /// An exact fixed-width integer result with explicit width and signedness.
    Integer(TypedInteger),
    /// A linearly dimensioned result with its units and approximation provenance.
    DimensionalQuantity(CalculatedQuantity),
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
            Self::Boolean(value) => Some(value.to_string()),
            Self::Sequence(value) => Some(value.display()),
            Self::Vector(value) => value.formatted_result(),
            Self::Integer(value) => Some(value.formatted_decimal()),
            Self::DimensionalQuantity(value) => value.formatted_result(),
            Self::PhysicalQuantity { value, unit, .. } => {
                Some(format!("{} {}", format_number(*value)?, unit.symbol()))
            }
            Self::ExactBase { digits, .. } => Some(digits.clone()),
        }
    }

    /// Complete parseable value for clipboard operations.
    pub fn canonical_result(&self) -> Option<String> {
        match self {
            Self::Sequence(value) => Some(value.canonical_copy().to_owned()),
            _ => self.formatted_result(),
        }
    }

    pub fn is_approximate(&self) -> bool {
        match self {
            Self::DimensionalQuantity(value) => value.is_approximate(),
            Self::PhysicalQuantity { approximations, .. } => !approximations.is_empty(),
            Self::Scalar(_) | Self::Boolean(_) | Self::Sequence(_) | Self::Vector(_) | Self::Integer(_) | Self::ExactBase { .. } => false,
        }
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
    Script(String),
    WorkLimit,
    Canceled,
    Vector(String),
    InvalidBaseNumber,
    InvalidPrefixedInteger {
        radix: u32,
    },
    PrefixedIntegerOverflow {
        radix: u32,
    },
    PrefixedIntegerTooLarge {
        max_exact: u64,
    },
    ScalarIntegerTooLarge {
        max_exact: u64,
    },
    InvalidScientificLiteral,
    ScientificLiteralOutOfRange,
    PreparedExpressionTooLong {
        max_bytes: usize,
    },
    UnitConversion(ConversionError),
    Quantity(QuantityError),
    ExpressionTooLong {
        max_bytes: usize,
    },
    NestingTooDeep {
        max_depth: usize,
    },
    Programmer(ProgrammerError),
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
            Kind::InvalidArithmetic => formatter
                .write_str("division or remainder by zero, or non-finite arithmetic result"),
            Kind::Script(message) => formatter.write_str(message),
            Kind::WorkLimit => formatter.write_str("evaluation work limit exceeded"),
            Kind::Canceled => formatter.write_str("evaluation canceled"),
            Kind::Vector(message) => write!(formatter, "{message}"),
            Kind::InvalidBaseNumber => formatter.write_str("invalid unsigned base value"),
            Kind::InvalidPrefixedInteger { radix } => {
                write!(formatter, "invalid base-{radix} integer literal")
            }
            Kind::PrefixedIntegerOverflow { radix } => write!(
                formatter,
                "base-{radix} integer literal exceeds the unsigned 128-bit limit"
            ),
            Kind::PrefixedIntegerTooLarge { max_exact } => write!(
                formatter,
                "integer literal exceeds the exactly representable scalar limit of {max_exact}"
            ),
            Kind::ScalarIntegerTooLarge { max_exact } => write!(
                formatter,
                "integer literal exceeds the exactly representable scalar limit of {max_exact}"
            ),
            Kind::InvalidScientificLiteral => {
                formatter.write_str("invalid scientific-notation literal")
            }
            Kind::ScientificLiteralOutOfRange => formatter
                .write_str("scientific-notation literal is outside the finite scalar range"),
            Kind::PreparedExpressionTooLong { max_bytes } => write!(
                formatter,
                "normalized expression exceeds the {max_bytes}-byte evaluation limit"
            ),
            Kind::UnitConversion(error) => write!(formatter, "{error}"),
            Kind::Quantity(error) => write!(formatter, "{error}"),
            Kind::ExpressionTooLong { max_bytes } => {
                write!(formatter, "expression exceeds the {max_bytes}-byte limit")
            }
            Kind::NestingTooDeep { max_depth } => {
                write!(formatter, "expression nesting exceeds {max_depth} levels")
            }
            Kind::Programmer(error) => error.fmt(formatter),
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
    Expression(Box<PreparedExpression>),
    Script(Box<script::ScriptExpression>),
    PhysicalConversion(String),
    PhysicalReference {
        destination: Unit,
        destination_text: String,
    },
    QuantityTarget {
        expression: QuantityExpression,
        target: UnitProduct,
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

#[derive(Debug)]
struct PreparedExpression {
    simple_reference: Option<String>,
    vector: Option<Result<VectorExpression, DiagnosticKind>>,
    explicit_vector: bool,
    scalar: Option<PreparedScalar>,
    scalar_error: Option<DiagnosticKind>,
    programmer: Option<ProgrammerExpression>,
    programmer_error: Option<ProgrammerError>,
    uses_programmer_syntax: bool,
    quantity: Option<QuantityExpression>,
    quantity_error: Option<QuantityError>,
    quantity_unit_evidence: UnitParseEvidence,
}

#[derive(Debug)]
pub(super) enum PreparedScalar {
    Checked(Box<CheckedExpression>),
    Modulo(Box<ModuloExpression>),
}

impl PreparedScalar {
    pub(super) fn var_names(&self) -> &[String] {
        match self {
            Self::Checked(expression) => expression.var_names(),
            Self::Modulo(expression) => expression.var_names(),
        }
    }

    pub(super) fn eval(&self, variables: &[f64]) -> exmex::ExResult<f64> {
        match self {
            Self::Checked(expression) => expression.eval(variables),
            Self::Modulo(expression) => expression.eval(variables),
        }
    }
}

fn scalar_from_scan(
    expression: &str,
    scan: &ExpressionScan,
) -> Result<PreparedScalar, DiagnosticKind> {
    if let Some(error) = &scan.decimal_error {
        return Err(error.clone());
    }
    let lowered_numbers = lower_numeric_literals(expression, &scan.numeric_literals)?;
    let lowered = if scan.uses_binary_function {
        Cow::Owned(lower_binary_function_calls(lowered_numbers.as_ref()))
    } else {
        lowered_numbers
    };
    if lowered.len() > MAX_PREPARED_EXPRESSION_BYTES {
        return Err(DiagnosticKind::PreparedExpressionTooLong {
            max_bytes: MAX_PREPARED_EXPRESSION_BYTES,
        });
    }
    if scan.uses_remainder || scan.uses_mod_function {
        ModuloExpression::parse_wo_compile(lowered.as_ref())
            .map(Box::new)
            .map(PreparedScalar::Modulo)
    } else {
        CheckedExpression::parse_wo_compile(lowered.as_ref())
            .map(Box::new)
            .map(PreparedScalar::Checked)
    }
    .map_err(|_| DiagnosticKind::InvalidExpression)
}

pub(super) fn prepare_vector_scalar(
    expression: &str,
    declared_names: &HashSet<String>,
) -> Result<PreparedScalar, DiagnosticKind> {
    let (normalized, _) = normalize_scalar_tokens(expression, declared_names);
    let scan = scan_expression(&normalized)?;
    scalar_from_scan(&normalized, &scan)
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

#[derive(Debug, Default)]
struct ExpressionScan {
    identifiers: usize,
    numeric_literals: Vec<NumericLiteralRewrite>,
    decimal_error: Option<DiagnosticKind>,
    uses_remainder: bool,
    uses_mod_function: bool,
    uses_binary_function: bool,
}

#[derive(Debug)]
struct NumericLiteralRewrite {
    source_range: Range<usize>,
    replacement: String,
}

fn is_full_line_comment(trimmed: &str) -> bool {
    trimmed.starts_with('#') || trimmed.starts_with("//")
}

fn prepare_line(
    source_range: Range<usize>,
    source_line: &str,
    total_identifiers: &mut usize,
    declared_names: &HashSet<String>,
) -> PreparedLine {
    prepare_line_internal(source_range, source_line, total_identifiers, declared_names, false)
}

fn prepare_line_internal(
    source_range: Range<usize>,
    source_line: &str,
    total_identifiers: &mut usize,
    declared_names: &HashSet<String>,
    internal_template: bool,
) -> PreparedLine {
    let trimmed = source_line.trim();
    if trimmed.is_empty() {
        return PreparedLine::simple(source_range, PreparedStatement::Blank);
    }
    if is_full_line_comment(trimmed) {
        return PreparedLine::simple(source_range, PreparedStatement::Comment);
    }

    let (assignment_name, expression, assignment_diagnostic) =
        match find_assignment_delimiter(trimmed) {
            Some(index) => {
                let left = trimmed[..index].trim();
                let right = trimmed[index + 1..].trim();
                match parse_assignment_name(left) {
                    Ok(assignment) => {
                        let diagnostic = if is_row_reference_name(&assignment.canonical)
                            || (!assignment.braced && is_reserved_name(&assignment.canonical))
                        {
                            Some(Diagnostic::new(DiagnosticKind::ReservedName(
                                assignment.canonical.clone(),
                            )))
                        } else if find_assignment_delimiter(right).is_some() {
                            Some(Diagnostic::new(DiagnosticKind::InvalidAssignment))
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
    let expression_limit = if internal_template { MAX_PREPARED_EXPRESSION_BYTES } else { MAX_EXPRESSION_BYTES };
    if expression.len() > expression_limit {
        return PreparedLine::invalid(
            source_range,
            assignment_name,
            DiagnosticKind::ExpressionTooLong {
                max_bytes: expression_limit,
            },
        );
    }

    if script::uses_script_syntax(expression) {
        let identifiers = match script::validate_source(expression) {
            Ok(count) => count,
            Err(kind) => return PreparedLine::invalid(source_range, assignment_name, kind),
        };
        if total_identifiers.saturating_add(identifiers) > MAX_DEPENDENCIES {
            return PreparedLine::invalid(source_range, assignment_name, DiagnosticKind::TooManyIdentifiers { max_identifiers: MAX_DEPENDENCIES });
        }
        *total_identifiers += identifiers;
        return match script::prepare(expression, declared_names) {
            Ok(expression) => PreparedLine {
                source_range, assignment_name,
                statement: PreparedStatement::Script(Box::new(expression)),
                references: Vec::new(), dependencies: Vec::new(), diagnostic: None,
            },
            Err(kind) => PreparedLine::invalid(source_range, assignment_name, kind),
        };
    }

    // Exact base conversions have their own u128 grammar. Dispatch them before
    // scalar-token scanning so hexadecimal digits such as `0bad` retain their
    // exact-conversion meaning and are not subject to scalar precision limits.
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

    let explicit_vector = vector::has_explicit_syntax(expression);
    let (normalized, programmer_input) = normalize_scalar_tokens(expression, declared_names);
    let programmer_parse = programmer::parse(&programmer_input);
    if let Some(ProgrammerError::NameTooLong { max_bytes }) = &programmer_parse.error {
        return PreparedLine::invalid(
            source_range,
            assignment_name,
            DiagnosticKind::NameTooLong {
                max_bytes: *max_bytes,
            },
        );
    }
    let scan = scan_expression(&normalized);
    if let Err(kind) = &scan {
        let typed_route_can_win = matches!(kind, DiagnosticKind::PrefixedIntegerTooLarge { .. })
            && programmer_parse
                .expression
                .as_ref()
                .is_some_and(|programmer| {
                    programmer_parse.uses_programmer_syntax
                        || programmer
                            .references()
                            .iter()
                            .any(|(name, braced)| *braced || !is_reserved_name(name))
                });
        if !typed_route_can_win {
            return PreparedLine::invalid(source_range, assignment_name, kind.clone());
        }
    }
    let identifier_count =
        if programmer_parse.expression.is_some() || programmer_parse.uses_programmer_syntax {
            programmer_parse.identifier_count
        } else {
            scan.as_ref().map_or(0, |scan| scan.identifiers)
        };
    if identifier_count > MAX_DEPENDENCIES_PER_LINE {
        return PreparedLine::invalid(
            source_range,
            assignment_name,
            DiagnosticKind::TooManyIdentifiers {
                max_identifiers: MAX_DEPENDENCIES_PER_LINE,
            },
        );
    }
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

    if explicit_vector {
        return match vector::prepare(&normalized, declared_names) {
            Ok(expression) => {
                let references = expression
                    .references()
                    .iter()
                    .map(|name| prepared_reference(name))
                    .collect();
                PreparedLine {
                    source_range,
                    assignment_name,
                    statement: PreparedStatement::Expression(Box::new(PreparedExpression {
                        simple_reference: None,
                        vector: Some(Ok(expression)),
                        explicit_vector: true,
                        scalar: None,
                        scalar_error: None,
                        programmer: None,
                        programmer_error: None,
                        uses_programmer_syntax: false,
                        quantity: None,
                        quantity_error: None,
                        quantity_unit_evidence: UnitParseEvidence::None,
                    })),
                    references,
                    dependencies: Vec::new(),
                    diagnostic: None,
                }
            }
            Err(kind) => PreparedLine::invalid(source_range, assignment_name, kind),
        };
    }

    // Feed the quantity parser the same validated numeric tokens as the
    // scalar parser. This permits a prefixed value next to a real unit while
    // keeping large exact/programmer expressions on their original path.
    let quantity_input = scan
        .as_ref()
        .ok()
        .and_then(|scan| lower_numeric_literals(&normalized, &scan.numeric_literals).ok())
        .unwrap_or(Cow::Borrowed(&normalized));

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
            if let Some(unit) = unit_conversion::unit_by_alias(destination) {
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
            return match quantity::parse_unit_target(destination) {
                Ok(target) => {
                    let attempt = quantity::parse_expression(source);
                    match attempt.expression {
                        Some(expression) => PreparedLine {
                            source_range,
                            assignment_name,
                            statement: PreparedStatement::QuantityTarget { expression, target },
                            references: attempt
                                .references
                                .iter()
                                .map(|name| prepared_reference(name))
                                .collect(),
                            dependencies: Vec::new(),
                            diagnostic: None,
                        },
                        None => PreparedLine::invalid(
                            source_range,
                            assignment_name,
                            DiagnosticKind::Quantity(
                                attempt.error.unwrap_or(QuantityError::InvalidSyntax),
                            ),
                        ),
                    }
                }
                Err(error) => PreparedLine::invalid(
                    source_range,
                    assignment_name,
                    DiagnosticKind::Quantity(error),
                ),
            };
        }

        if unit_conversion::parse_conversion(expression).is_ok() {
            return PreparedLine {
                source_range,
                assignment_name,
                statement: PreparedStatement::PhysicalConversion(expression.to_owned()),
                references: Vec::new(),
                dependencies: Vec::new(),
                diagnostic: None,
            };
        }

        let (quantity_source, _) = split_to_keyword(&quantity_input).expect("split matched above");
        let attempt = quantity::parse_expression(quantity_source);
        if attempt.unit_evidence.has_unit() {
            let target = match quantity::parse_unit_target(destination) {
                Ok(target) => target,
                Err(error) => {
                    return PreparedLine::invalid(
                        source_range,
                        assignment_name,
                        DiagnosticKind::Quantity(error),
                    );
                }
            };
            let Some(quantity_expression) = attempt.expression else {
                return PreparedLine::invalid(
                    source_range,
                    assignment_name,
                    DiagnosticKind::Quantity(attempt.error.unwrap_or(QuantityError::InvalidSyntax)),
                );
            };
            return PreparedLine {
                source_range,
                assignment_name,
                statement: PreparedStatement::QuantityTarget {
                    expression: quantity_expression,
                    target,
                },
                references: attempt
                    .references
                    .iter()
                    .map(|name| prepared_reference(name))
                    .collect(),
                dependencies: Vec::new(),
                diagnostic: None,
            };
        }
        if !attempt.references.is_empty()
            && let (Some(quantity_expression), Ok(target)) =
                (attempt.expression, quantity::parse_unit_target(destination))
        {
            return PreparedLine {
                source_range,
                assignment_name,
                statement: PreparedStatement::QuantityTarget {
                    expression: quantity_expression,
                    target,
                },
                references: attempt
                    .references
                    .iter()
                    .map(|name| prepared_reference(name))
                    .collect(),
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

    let quantity_parse = quantity::parse_expression(&quantity_input);
    let quantity_unit_evidence = quantity_parse.unit_evidence;
    if quantity_unit_evidence == UnitParseEvidence::Definitive
        && !programmer_parse.uses_programmer_syntax
    {
        let Some(quantity) = quantity_parse.expression else {
            return PreparedLine::invalid(
                source_range,
                assignment_name,
                DiagnosticKind::Quantity(
                    quantity_parse.error.unwrap_or(QuantityError::InvalidSyntax),
                ),
            );
        };
        let references = quantity_parse
            .references
            .iter()
            .map(|name| prepared_reference(name))
            .collect();
        return PreparedLine {
            source_range,
            assignment_name,
            statement: PreparedStatement::Expression(Box::new(PreparedExpression {
                simple_reference: None,
                vector: None,
                explicit_vector: false,
                scalar: None,
                scalar_error: None,
                programmer: None,
                programmer_error: None,
                uses_programmer_syntax: false,
                quantity: Some(quantity),
                quantity_error: None,
                quantity_unit_evidence,
            })),
            references,
            dependencies: Vec::new(),
            diagnostic: None,
        };
    }

    let scalar_result = scan.and_then(|scan| scalar_from_scan(&normalized, &scan));
    let (scalar, scalar_error) = match scalar_result {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };

    let programmer = programmer_parse.expression;
    let uses_programmer_syntax = programmer_parse.uses_programmer_syntax;
    let programmer_error = programmer_parse.error;
    if quantity_unit_evidence == UnitParseEvidence::PlainMinuteAlias
        && !uses_programmer_syntax
        && scalar.is_none()
    {
        if let Some(quantity) = quantity_parse.expression {
            let references = quantity_parse
                .references
                .iter()
                .map(|name| prepared_reference(name))
                .collect();
            return PreparedLine {
                source_range,
                assignment_name,
                statement: PreparedStatement::Expression(Box::new(PreparedExpression {
                    simple_reference: None,
                    vector: None,
                    explicit_vector: false,
                    scalar: None,
                    scalar_error: None,
                    programmer: None,
                    programmer_error: None,
                    uses_programmer_syntax: false,
                    quantity: Some(quantity),
                    quantity_error: None,
                    quantity_unit_evidence,
                })),
                references,
                dependencies: Vec::new(),
                diagnostic: None,
            };
        }
        return PreparedLine::invalid(
            source_range,
            assignment_name,
            DiagnosticKind::Quantity(quantity_parse.error.unwrap_or(QuantityError::InvalidSyntax)),
        );
    }
    if scalar.is_none() && programmer.is_none() {
        let diagnostic = if uses_programmer_syntax {
            DiagnosticKind::Programmer(programmer_error.unwrap_or(ProgrammerError::InvalidSyntax))
        } else {
            scalar_error.unwrap_or(DiagnosticKind::InvalidExpression)
        };
        return PreparedLine::invalid(source_range, assignment_name, diagnostic);
    }

    // Only reference-bearing expressions can dynamically acquire vector
    // operands. Avoid building a second tree for ordinary numeric results.
    let has_candidate_reference = scalar
        .as_ref()
        .is_some_and(|value| !value.var_names().is_empty())
        || programmer.as_ref().is_some_and(|value| {
            value.references().iter().any(|(name, braced)| {
                *braced || (!is_reserved_name(name) && !is_unbound_pi_alias(name, declared_names))
            })
        });
    let vector = has_candidate_reference.then(|| vector::prepare(&normalized, declared_names));

    let mut references = Vec::new();
    if let Some(programmer) = &programmer {
        references.extend(
            programmer
                .references()
                .into_iter()
                .filter(|(name, braced)| {
                    *braced
                        || (!is_reserved_name(name) && !is_unbound_pi_alias(name, declared_names))
                })
                .map(|(name, _)| prepared_reference(&name)),
        );
    }
    if let Some(scalar) = &scalar {
        references.extend(
            scalar
                .var_names()
                .iter()
                .map(|name| prepared_reference(name)),
        );
    }
    if let Some(Ok(vector)) = &vector {
        for name in vector.references() {
            if !references.iter().any(|reference| reference.name == *name) {
                references.push(prepared_reference(name));
            }
        }
    }
    if quantity_unit_evidence == UnitParseEvidence::PlainMinuteAlias {
        for name in &quantity_parse.references {
            if !references.iter().any(|reference| reference.name == *name) {
                references.push(prepared_reference(name));
            }
        }
    }
    PreparedLine {
        source_range,
        assignment_name,
        statement: PreparedStatement::Expression(Box::new(PreparedExpression {
            simple_reference: parse_simple_reference(expression).map(|reference| reference.name),
            vector,
            explicit_vector: false,
            scalar,
            scalar_error,
            programmer,
            programmer_error,
            uses_programmer_syntax,
            quantity: quantity_parse.expression,
            quantity_error: quantity_parse.error,
            quantity_unit_evidence,
        })),
        references,
        dependencies: Vec::new(),
        diagnostic: None,
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
            '=' if !in_braces => {
                let before = expression[..index].chars().next_back();
                let after = expression[index + 1..].chars().next();
                if !matches!(before, Some('=' | '!' | '<' | '>')) && after != Some('=') {
                    return Some(index);
                }
            }
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

fn declared_names(source: &str) -> HashSet<String> {
    source
        .split('\n')
        .filter_map(|line| {
            let line = line.trim();
            if is_full_line_comment(line) {
                return None;
            }
            let index = find_assignment_delimiter(line)?;
            let assignment = parse_assignment_name(line[..index].trim()).ok()?;
            if is_row_reference_name(&assignment.canonical)
                || (!assignment.braced && is_reserved_name(&assignment.canonical))
            {
                return None;
            }
            Some(assignment.canonical)
        })
        .collect()
}

fn is_unbound_pi_alias(name: &str, declared_names: &HashSet<String>) -> bool {
    name != "PI"
        && (name.eq_ignore_ascii_case("pi") || name.eq_ignore_ascii_case("pie"))
        && !declared_names.contains(name)
}

/// Normalize only tokens whose syntactic position gives them a built-in meaning.
/// The programmer form changes only infix `mod`; literals and other names
/// remain byte-for-byte intact for the exact integer parser.
fn normalize_scalar_tokens(expression: &str, declared_names: &HashSet<String>) -> (String, String) {
    let mut output = String::with_capacity(expression.len());
    let mut programmer_output = String::with_capacity(expression.len());
    let mut chars = expression.char_indices().peekable();
    let mut copied_until = 0;
    let mut programmer_copied_until = 0;
    let mut can_end_operand = false;
    while let Some((start, character)) = chars.next() {
        if character == '{' {
            for (_, next) in chars.by_ref() {
                if next == '}' {
                    break;
                }
            }
            can_end_operand = true;
            continue;
        }
        if !is_identifier_start(character) {
            if character.is_ascii_digit() || character == ')' {
                can_end_operand = true;
            } else if !character.is_whitespace() && character != '.' {
                can_end_operand = false;
            }
            continue;
        }
        // A name cannot begin in the middle of a number or identifier.
        if expression[..start]
            .chars()
            .next_back()
            .is_some_and(is_identifier_continue)
        {
            continue;
        }
        let mut end = start + character.len_utf8();
        while let Some((_, next)) = chars.peek().copied() {
            if !is_identifier_continue(next) {
                break;
            }
            end += next.len_utf8();
            chars.next();
        }
        let name = &expression[start..end];
        let next = expression[end..]
            .chars()
            .find(|character| !character.is_whitespace());
        let infix_mod = name.eq_ignore_ascii_case("mod")
            && can_end_operand
            && next.is_some_and(|character| {
                is_identifier_start(character)
                    || character.is_ascii_digit()
                    || matches!(character, '(' | '{' | '+' | '-' | '.')
            });
        let lowercase = name.to_ascii_lowercase();
        let function = next == Some('(')
            && matches!(
                lowercase.as_str(),
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
                    | "mod"
            );
        let replacement = if infix_mod {
            programmer_output.push_str(&expression[programmer_copied_until..start]);
            programmer_output.push('%');
            programmer_copied_until = end;
            Some("%")
        } else if function {
            Some(lowercase.as_str())
        } else if is_unbound_pi_alias(name, declared_names) {
            Some("PI")
        } else {
            None
        };
        if let Some(replacement) = replacement {
            output.push_str(&expression[copied_until..start]);
            output.push_str(replacement);
            copied_until = end;
        }
        can_end_operand = !infix_mod;
    }
    output.push_str(&expression[copied_until..]);
    programmer_output.push_str(&expression[programmer_copied_until..]);
    (output, programmer_output)
}

fn scan_expression(expression: &str) -> Result<ExpressionScan, DiagnosticKind> {
    let mut scan = ExpressionScan::default();
    let mut depth = 0;
    let mut characters = expression.char_indices().peekable();

    while let Some((index, character)) = characters.next() {
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
                scan.identifiers += 1;
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
            '%' => scan.uses_remainder = true,
            // `@` is reserved for the private lowering of mod(a, b). It was not
            // part of the user grammar before this checkpoint.
            '@' => return Err(DiagnosticKind::InvalidExpression),
            '0'..='9' | '.' => {
                let literal = if character == '0' {
                    parse_prefixed_integer(expression, index)?
                } else {
                    None
                }
                .or(parse_scientific_literal(expression, index)?);
                if let Some(literal) = literal {
                    while characters
                        .peek()
                        .is_some_and(|(next_index, _)| *next_index < literal.source_range.end)
                    {
                        characters.next();
                    }
                    scan.numeric_literals.push(literal);
                } else if character.is_ascii_digit()
                    && let Err(error) = validate_decimal_integer(expression, index)
                    && scan.decimal_error.is_none()
                {
                    scan.decimal_error = Some(error);
                }
            }
            character if is_identifier_start(character) => {
                let start = index;
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
                scan.identifiers += 1;
                let name = &expression[start..start + name_bytes];
                if binary_function_operator(name).is_some()
                    && next_non_whitespace_index(expression, start + name_bytes)
                        .is_some_and(|open| expression[open..].starts_with('('))
                {
                    scan.uses_binary_function = true;
                    scan.uses_mod_function |= name == "mod";
                }
            }
            _ => {}
        }
        if scan.identifiers > MAX_DEPENDENCIES_PER_LINE {
            return Err(DiagnosticKind::TooManyIdentifiers {
                max_identifiers: MAX_DEPENDENCIES_PER_LINE,
            });
        }
    }
    if depth != 0 {
        return Err(DiagnosticKind::InvalidExpression);
    }
    Ok(scan)
}

fn validate_decimal_integer(expression: &str, start: usize) -> Result<(), DiagnosticKind> {
    if expression[..start]
        .chars()
        .next_back()
        .is_some_and(|previous| is_identifier_continue(previous) || previous == '.')
    {
        return Ok(());
    }
    let bytes = expression.as_bytes();
    let mut end = start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    // Decimal points and scientific exponents deliberately select floating
    // syntax. Identifier/unit suffixes remain owned by their existing parser.
    if expression[end..]
        .chars()
        .next()
        .is_some_and(|next| next == '.' || is_identifier_continue(next))
    {
        return Ok(());
    }
    let value = expression[start..end].parse::<u128>();
    if value.is_err() || value.is_ok_and(|value| value > u128::from(MAX_EXACT_SCALAR_INTEGER)) {
        return Err(DiagnosticKind::ScalarIntegerTooLarge {
            max_exact: MAX_EXACT_SCALAR_INTEGER,
        });
    }
    Ok(())
}

fn parse_prefixed_integer(
    expression: &str,
    start: usize,
) -> Result<Option<NumericLiteralRewrite>, DiagnosticKind> {
    if expression[..start]
        .chars()
        .next_back()
        .is_some_and(|previous| is_identifier_continue(previous) || previous == '.')
    {
        return Ok(None);
    }

    let Some(prefix) = expression.as_bytes().get(start + 1).copied() else {
        return Ok(None);
    };
    let (base, radix) = match prefix.to_ascii_lowercase() {
        b'x' => (NumericBase::Hexadecimal, 16),
        b'b' => (NumericBase::Binary, 2),
        b'o' => (NumericBase::Octal, 8),
        _ => return Ok(None),
    };

    let mut end = start + 2;
    let digits_start = end;
    for (offset, character) in expression[digits_start..].char_indices() {
        if !is_identifier_continue(character) && character != '.' {
            break;
        }
        end = digits_start + offset + character.len_utf8();
    }

    let digits = &expression[start + 2..end];
    if digits.is_empty()
        || !digits
            .bytes()
            .all(|digit| digit_is_valid_for_radix(digit, radix))
    {
        return Err(DiagnosticKind::InvalidPrefixedInteger { radix });
    }
    let value =
        parse_unsigned(digits, base).ok_or(DiagnosticKind::PrefixedIntegerOverflow { radix })?;
    if value > u128::from(MAX_EXACT_SCALAR_INTEGER) {
        return Err(DiagnosticKind::PrefixedIntegerTooLarge {
            max_exact: MAX_EXACT_SCALAR_INTEGER,
        });
    }

    Ok(Some(NumericLiteralRewrite {
        source_range: start..end,
        replacement: (value as u64).to_string(),
    }))
}

fn parse_scientific_literal(
    expression: &str,
    start: usize,
) -> Result<Option<NumericLiteralRewrite>, DiagnosticKind> {
    if expression[..start]
        .chars()
        .next_back()
        .is_some_and(|previous| is_identifier_continue(previous) || previous == '.')
    {
        return Ok(None);
    }

    let bytes = expression.as_bytes();
    let mut cursor = start;
    let mut mantissa_digits = 0;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
        mantissa_digits += 1;
    }
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
            mantissa_digits += 1;
        }
    }
    if mantissa_digits == 0
        || !bytes
            .get(cursor)
            .is_some_and(|byte| matches!(*byte, b'e' | b'E'))
    {
        return Ok(None);
    }

    cursor += 1;
    if bytes
        .get(cursor)
        .is_some_and(|byte| matches!(*byte, b'+' | b'-'))
    {
        cursor += 1;
    }
    let exponent_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    if cursor == exponent_start {
        return Err(DiagnosticKind::InvalidScientificLiteral);
    }

    let value = expression[start..cursor]
        .parse::<f64>()
        .map_err(|_| DiagnosticKind::ScientificLiteralOutOfRange)?;
    if !value.is_finite() {
        return Err(DiagnosticKind::ScientificLiteralOutOfRange);
    }

    Ok(Some(NumericLiteralRewrite {
        source_range: start..cursor,
        replacement: decimalize_finite_float(value),
    }))
}

fn decimalize_finite_float(value: f64) -> String {
    let representation = value.to_string();
    let Some((mantissa, exponent)) = representation.split_once(['e', 'E']) else {
        return representation;
    };
    let exponent = exponent.parse::<i32>().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.strip_prefix('-').unwrap_or(mantissa);
    let decimal_position = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let digits = mantissa.replace('.', "");
    let final_position = decimal_position + exponent;
    let mut decimal = String::with_capacity(
        digits
            .len()
            .saturating_add(exponent.unsigned_abs() as usize + 2),
    );
    if negative {
        decimal.push('-');
    }
    if final_position <= 0 {
        decimal.push_str("0.");
        decimal.extend(std::iter::repeat_n(
            '0',
            final_position.unsigned_abs() as usize,
        ));
        decimal.push_str(&digits);
    } else if final_position as usize >= digits.len() {
        decimal.push_str(&digits);
        decimal.extend(std::iter::repeat_n(
            '0',
            final_position as usize - digits.len(),
        ));
    } else {
        let split = final_position as usize;
        decimal.push_str(&digits[..split]);
        decimal.push('.');
        decimal.push_str(&digits[split..]);
    }
    decimal
}

fn digit_is_valid_for_radix(digit: u8, radix: u32) -> bool {
    let value = match digit {
        b'0'..=b'9' => u32::from(digit - b'0'),
        b'a'..=b'f' => u32::from(digit - b'a') + 10,
        b'A'..=b'F' => u32::from(digit - b'A') + 10,
        _ => return false,
    };
    value < radix
}

fn lower_numeric_literals<'a>(
    expression: &'a str,
    literals: &[NumericLiteralRewrite],
) -> Result<Cow<'a, str>, DiagnosticKind> {
    if literals.is_empty() {
        return Ok(Cow::Borrowed(expression));
    }

    let mut lowered_len = expression.len();
    for literal in literals {
        lowered_len = lowered_len
            .checked_sub(literal.source_range.len())
            .and_then(|length| length.checked_add(literal.replacement.len()))
            .ok_or(DiagnosticKind::PreparedExpressionTooLong {
                max_bytes: MAX_PREPARED_EXPRESSION_BYTES,
            })?;
    }
    if lowered_len > MAX_PREPARED_EXPRESSION_BYTES {
        return Err(DiagnosticKind::PreparedExpressionTooLong {
            max_bytes: MAX_PREPARED_EXPRESSION_BYTES,
        });
    }

    let mut lowered = String::with_capacity(lowered_len);
    let mut copied_until = 0;
    for literal in literals {
        lowered.push_str(&expression[copied_until..literal.source_range.start]);
        lowered.push_str(&literal.replacement);
        copied_until = literal.source_range.end;
    }
    lowered.push_str(&expression[copied_until..]);
    Ok(Cow::Owned(lowered))
}

fn binary_function_operator(name: &str) -> Option<&'static str> {
    match name {
        "mod" => Some("@"),
        "min" => Some("min"),
        "max" => Some("max"),
        "atan2" => Some("atan2"),
        _ => None,
    }
}

// Exmex supports these operators in infix form, but its comma rewrite cannot
// reliably associate nested binary calls with their matching function.
fn lower_binary_function_calls(expression: &str) -> String {
    fn lower(source: &str) -> String {
        let mut output = String::with_capacity(source.len());
        let mut characters = source.char_indices().peekable();
        let mut copied_until = 0;

        while let Some((start, character)) = characters.next() {
            if character == '{' {
                for (_, next) in characters.by_ref() {
                    if next == '}' {
                        break;
                    }
                }
                continue;
            }
            if !is_identifier_start(character) {
                continue;
            }

            let mut end = start + character.len_utf8();
            while let Some((_, next)) = characters.peek().copied() {
                if !is_identifier_continue(next) {
                    break;
                }
                end += next.len_utf8();
                characters.next();
            }
            let Some(operator) = binary_function_operator(&source[start..end]) else {
                continue;
            };
            let Some(open) = next_non_whitespace_index(source, end) else {
                continue;
            };
            if !source[open..].starts_with('(') {
                continue;
            }
            let Some((close, Some(comma), false)) = binary_call_delimiters(source, open) else {
                continue;
            };

            let left = source[open + 1..comma].trim();
            let right = source[comma + 1..close].trim();
            if left.is_empty() || right.is_empty() {
                continue;
            }

            output.push_str(&source[copied_until..start]);
            output.push_str("((");
            output.push_str(&lower(left));
            output.push_str(") ");
            output.push_str(operator);
            output.push_str(" (");
            output.push_str(&lower(right));
            output.push_str("))");
            copied_until = close + 1;
            while characters
                .peek()
                .is_some_and(|(next_index, _)| *next_index < copied_until)
            {
                characters.next();
            }
        }

        output.push_str(&source[copied_until..]);
        output
    }

    lower(expression)
}

fn binary_call_delimiters(expression: &str, open: usize) -> Option<(usize, Option<usize>, bool)> {
    let mut depth = 0;
    let mut in_braces = false;
    let mut comma = None;
    let mut multiple_commas = false;

    for (offset, character) in expression[open..].char_indices() {
        let index = open + offset;
        match character {
            '{' => in_braces = true,
            '}' => in_braces = false,
            '(' if !in_braces => depth += 1,
            ')' if !in_braces => {
                depth -= 1;
                if depth == 0 {
                    return Some((index, comma, multiple_commas));
                }
            }
            ',' if !in_braces && depth == 1 && comma.replace(index).is_some() => {
                multiple_commas = true;
            }
            _ => {}
        }
    }
    None
}

fn next_non_whitespace_index(expression: &str, start: usize) -> Option<usize> {
    for (offset, character) in expression[start..].char_indices() {
        if !character.is_whitespace() {
            return Some(start + offset);
        }
    }
    None
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
        if let Some(name) = &line.assignment_name
            && !matches!(
                line.diagnostic.as_ref().map(|diagnostic| &diagnostic.kind),
                Some(DiagnosticKind::ReservedName(_))
            )
        {
            definitions
                .entry(name.clone())
                .or_default()
                .push(line_index);
        }
    }
    definitions
}

fn resolve_references(lines: &mut [PreparedLine], definitions: &HashMap<String, Vec<usize>>) {
    let row_count = lines.len();
    for (line_index, line) in lines.iter_mut().enumerate() {
        if let PreparedStatement::Script(expression) = &mut line.statement {
            expression.resolve(line_index, row_count, |name| {
                definitions.get(name).and_then(|locations| locations.iter().rev().find(|&&location| location < line_index).or_else(|| locations.first()).copied())
            });
        }
        resolve_line_references(line, row_count, |name| {
            definitions.get(name).and_then(|locations| {
                locations
                    .iter()
                    .rev()
                    .find(|&&location| location < line_index)
                    .or_else(|| locations.first())
                    .copied()
            })
        });
    }
}

fn resolve_line_references(
    line: &mut PreparedLine,
    row_count: usize,
    mut named_binding: impl FnMut(&str) -> Option<usize>,
) {
    let defer_scalar_diagnostic = defers_scalar_diagnostic(line);
    for reference_index in 0..line.references.len() {
        let reference_name = line.references[reference_index].name.clone();
        let target = match line.references[reference_index].kind {
            ReferenceKind::Row => line.references[reference_index]
                .target_line
                .filter(|target| *target < row_count),
            ReferenceKind::InvalidRow => {
                if !defer_scalar_diagnostic {
                    set_diagnostic_if_missing(
                        line,
                        DiagnosticKind::UnknownRowReference(reference_name),
                    );
                }
                None
            }
            ReferenceKind::Named => match named_binding(&reference_name) {
                Some(location) => Some(location),
                None => {
                    if !defer_scalar_diagnostic {
                        set_diagnostic_if_missing(
                            line,
                            DiagnosticKind::UnknownName(reference_name),
                        );
                    }
                    None
                }
            },
        };
        if line.references[reference_index].kind == ReferenceKind::Row && target.is_none() {
            let name = line.references[reference_index].name.clone();
            if !defer_scalar_diagnostic {
                set_diagnostic_if_missing(line, DiagnosticKind::UnknownRowReference(name));
            }
        }
        line.references[reference_index].target_line = target;
        if let Some(target) = target {
            line.dependencies.push(target);
        }
    }
    line.dependencies.sort_unstable();
    line.dependencies.dedup();
}

/// Evaluate an independent expression against already-evaluated physical rows.
pub(super) fn evaluate_comparison_expression(
    expression: &str,
    rows: &[super::comparison::ComparisonRow],
    final_bindings: &HashMap<String, usize>,
) -> LineOutcome {
    if expression.contains(['\n', '\r']) || find_assignment_delimiter(expression).is_some() {
        return LineOutcome::Error(Diagnostic::new(DiagnosticKind::InvalidExpression));
    }
    let declared_names = final_bindings.keys().cloned().collect();
    let mut identifiers = 0;
    let mut prepared = prepare_line(
        0..expression.len(),
        expression,
        &mut identifiers,
        &declared_names,
    );
    resolve_line_references(&mut prepared, rows.len(), |name| {
        final_bindings.get(name).copied()
    });
    if let PreparedStatement::Script(expression) = &mut prepared.statement {
        expression.resolve(rows.len(), rows.len(), |name| final_bindings.get(name).copied());
    }
    let outcomes = rows
        .iter()
        .map(|row| row.outcome.clone())
        .collect::<Vec<_>>();
    let values = rows
        .iter()
        .map(|row| match &row.outcome {
            LineOutcome::Value(value) => Some(value.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if let PreparedStatement::Script(expression) = &prepared.statement {
        return match expression.evaluate(rows.len(), &outcomes, &values, &mut Vec::new(), &mut script::WorkBudget::default()) {
            Ok(value) => LineOutcome::Value(value),
            Err(kind) => LineOutcome::Error(Diagnostic::new(kind)),
        };
    }
    evaluate_line(
        rows.len(),
        &prepared,
        &vec![false; rows.len() + 1],
        &outcomes,
        &values,
    )
}

fn defers_scalar_diagnostic(line: &PreparedLine) -> bool {
    matches!(
        &line.statement,
        PreparedStatement::Expression(expression)
            if matches!(expression.as_ref(), PreparedExpression {
                programmer: Some(_),
                scalar_error: Some(_),
                uses_programmer_syntax: false,
                ..
            })
    )
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

    if let PreparedStatement::Expression(expression) = &line.statement
        && defers_scalar_diagnostic(line)
    {
        let typed_route_can_win = !line.references.is_empty()
            && line.references.iter().all(|reference| {
                reference
                    .target_line
                    .and_then(|dependency| values.get(dependency))
                    .and_then(Option::as_ref)
                    .is_some_and(|value| {
                        matches!(
                            value,
                            CalculationValue::Integer(_)
                                | CalculationValue::ExactBase { .. }
                                | CalculationValue::Vector(_)
                        )
                    })
            });
        let quantity_route_can_win = matches!(
            expression.scalar_error,
            Some(DiagnosticKind::ScalarIntegerTooLarge { .. })
        ) && line.references.iter().any(|reference| {
            reference
                .target_line
                .and_then(|index| values.get(index))
                .and_then(Option::as_ref)
                .is_some_and(|value| {
                    matches!(
                        value,
                        CalculationValue::PhysicalQuantity { .. }
                            | CalculationValue::DimensionalQuantity(_)
                    )
                })
        });
        if !typed_route_can_win
            && !quantity_route_can_win
            && let Some(error) = &expression.scalar_error
        {
            return LineOutcome::Error(Diagnostic::new(error.clone()));
        }
    }

    match &line.statement {
        PreparedStatement::Blank => return LineOutcome::Blank,
        PreparedStatement::Comment => return LineOutcome::Comment,
        PreparedStatement::Invalid => {
            return LineOutcome::Error(Diagnostic::new(DiagnosticKind::InvalidExpression));
        }
        PreparedStatement::Script(_) => unreachable!("script lines are evaluated by the script executor"),
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

    let result = evaluate_prepared_statement(&line.statement, line, values);

    match result {
        Ok(value) => LineOutcome::Value(value),
        Err(kind) => LineOutcome::Error(Diagnostic::new(kind)),
    }
}

fn evaluate_prepared_statement(
    statement: &PreparedStatement,
    line: &PreparedLine,
    values: &[Option<CalculationValue>],
) -> Result<CalculationValue, DiagnosticKind> {
    match statement {
        PreparedStatement::Blank | PreparedStatement::Comment | PreparedStatement::Invalid | PreparedStatement::Script(_) => {
            unreachable!("non-result line states returned above")
        }
        PreparedStatement::Expression(expression) => {
            evaluate_prepared_expression(expression, line, values)
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
            match values[dependency_line].as_ref() {
                Some(CalculationValue::PhysicalQuantity {
                    value,
                    unit,
                    approximations: prior_approximations,
                }) => {
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
                Some(CalculationValue::DimensionalQuantity(quantity)) => {
                    quantity::convert_to_unit(quantity, *destination)
                        .map(CalculationValue::DimensionalQuantity)
                        .map_err(DiagnosticKind::Quantity)
                }
                Some(_) => Err(DiagnosticKind::ExpectedPhysicalQuantity(
                    reference.name.clone(),
                )),
                None => Err(DiagnosticKind::DependencyFailed { dependency_line }),
            }
        }
        PreparedStatement::QuantityTarget {
            expression, target, ..
        } => {
            let bindings = quantity_bindings(line, values);
            expression
                .evaluate(&bindings, true)
                .and_then(|value| match value {
                    CalculationValue::DimensionalQuantity(quantity) => {
                        quantity::convert_to_target(&quantity, target)
                            .map(CalculationValue::DimensionalQuantity)
                    }
                    _ => Err(QuantityError::IncompatibleDimensions),
                })
                .map_err(DiagnosticKind::Quantity)
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
    }
}

fn evaluate_prepared_expression(
    expression: &PreparedExpression,
    line: &PreparedLine,
    values: &[Option<CalculationValue>],
) -> Result<CalculationValue, DiagnosticKind> {
    if let Some(name) = &expression.simple_reference
        && let Some(reference) = line.references.iter().find(|reference| &reference.name == name)
        && let Some(value) = reference.target_line.and_then(|index| values.get(index)).and_then(Option::as_ref)
        && matches!(value, CalculationValue::Boolean(_) | CalculationValue::Sequence(_))
    {
        return Ok(value.clone());
    }
    let has_vector_reference = line.references.iter().any(|reference| {
        reference
            .target_line
            .and_then(|index| values.get(index))
            .and_then(Option::as_ref)
            .is_some_and(|value| matches!(value, CalculationValue::Vector(_)))
    });
    if expression.explicit_vector || has_vector_reference {
        let vector = expression
            .vector
            .as_ref()
            .ok_or_else(|| DiagnosticKind::Vector("unsupported vector expression".into()))?;
        let vector = vector.as_ref().map_err(Clone::clone)?;
        return vector.evaluate(&quantity_bindings(line, values));
    }
    let has_quantity_reference = line.references.iter().any(|reference| {
        reference
            .target_line
            .and_then(|line_index| values.get(line_index))
            .and_then(Option::as_ref)
            .is_some_and(|value| {
                matches!(
                    value,
                    CalculationValue::PhysicalQuantity { .. }
                        | CalculationValue::DimensionalQuantity(_)
                )
            })
    });
    let ambiguous_minute_requires_quantity = expression.quantity_unit_evidence
        == UnitParseEvidence::PlainMinuteAlias
        && (expression.scalar.is_none()
            || !line.references.iter().all(|reference| {
                reference
                    .target_line
                    .and_then(|line_index| values.get(line_index))
                    .and_then(Option::as_ref)
                    .is_some_and(|value| matches!(value, CalculationValue::Scalar(_)))
            }));
    if !expression.uses_programmer_syntax
        && (expression.quantity_unit_evidence == UnitParseEvidence::Definitive
            || has_quantity_reference
            || ambiguous_minute_requires_quantity)
    {
        let Some(quantity) = expression.quantity.as_ref() else {
            return Err(DiagnosticKind::Quantity(
                expression
                    .quantity_error
                    .clone()
                    .unwrap_or(QuantityError::InvalidSyntax),
            ));
        };
        let bindings = quantity_bindings(line, values);
        return quantity
            .evaluate(&bindings, true)
            .map_err(DiagnosticKind::Quantity);
    }

    let has_typed_reference = line.references.iter().any(|reference| {
        reference
            .target_line
            .and_then(|line_index| values.get(line_index))
            .and_then(Option::as_ref)
            .is_some_and(|value| {
                matches!(
                    value,
                    CalculationValue::Integer(_) | CalculationValue::ExactBase { .. }
                )
            })
    });

    if expression.uses_programmer_syntax || has_typed_reference {
        let programmer = expression.programmer.as_ref().ok_or_else(|| {
            DiagnosticKind::Programmer(
                expression
                    .programmer_error
                    .clone()
                    .unwrap_or(ProgrammerError::InvalidSyntax),
            )
        })?;
        let mut bindings = HashMap::with_capacity(line.references.len());
        for reference in &line.references {
            if let Some(line_index) = reference.target_line
                && let Some(Some(value)) = values.get(line_index)
            {
                bindings.insert(reference.name.clone(), value.clone());
            }
        }
        return programmer
            .evaluate(&bindings)
            .map(CalculationValue::Integer)
            .map_err(DiagnosticKind::Programmer);
    }

    if let Some(error) = &expression.scalar_error {
        return Err(error.clone());
    }
    let Some(expression) = expression.scalar.as_ref() else {
        return Err(DiagnosticKind::InvalidExpression);
    };

    let mut variables = Vec::with_capacity(expression.var_names().len());
    for name in expression.var_names() {
        let Some(reference) = line
            .references
            .iter()
            .find(|reference| reference.name == *name)
        else {
            return Err(DiagnosticKind::UnknownName(name.clone()));
        };
        let Some(dependency_line) = reference.target_line else {
            return Err(DiagnosticKind::UnknownName(name.clone()));
        };
        match values.get(dependency_line).and_then(Option::as_ref) {
            Some(CalculationValue::Scalar(value)) => variables.push(*value),
            Some(_) => return Err(DiagnosticKind::ExpectedScalar(name.clone())),
            None => return Err(DiagnosticKind::DependencyFailed { dependency_line }),
        }
    }
    match expression.eval(&variables) {
        Ok(value) if value.is_finite() => Ok(CalculationValue::Scalar(value)),
        Ok(_) => Err(DiagnosticKind::InvalidArithmetic),
        Err(_) => Err(DiagnosticKind::InvalidExpression),
    }
}

fn quantity_bindings(
    line: &PreparedLine,
    values: &[Option<CalculationValue>],
) -> HashMap<String, CalculationValue> {
    let mut bindings = HashMap::with_capacity(line.references.len());
    for reference in &line.references {
        if let Some(value) = reference
            .target_line
            .and_then(|line_index| values.get(line_index))
            .and_then(Option::as_ref)
        {
            bindings.insert(reference.name.clone(), value.clone());
        }
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::{
        CalculationDocument, CalculationEvaluation, CalculationValue, DiagnosticKind, LineOutcome,
        MAX_DEPENDENCIES, MAX_DEPENDENCIES_PER_LINE, MAX_DOCUMENT_BYTES, MAX_EXPRESSION_BYTES,
        MAX_EXPRESSION_NESTING, MAX_NAME_BYTES, MAX_SOURCE_LINES, QuantityError,
    };
    use crate::calculation_pad::{ProgrammerError, TypedInteger};
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

    fn error_message(evaluation: &CalculationEvaluation<'_>, line_index: usize) -> String {
        match &evaluation.lines[line_index].outcome {
            LineOutcome::Error(diagnostic) => diagnostic.to_string(),
            other => panic!("expected an error on line {line_index}, got {other:?}"),
        }
    }

    fn scalar(evaluation: &CalculationEvaluation<'_>, line_index: usize) -> f64 {
        match value(evaluation, line_index) {
            CalculationValue::Scalar(value) => *value,
            other => panic!("expected a scalar on line {line_index}, got {other:?}"),
        }
    }

    fn integer(evaluation: &CalculationEvaluation<'_>, line_index: usize) -> TypedInteger {
        match value(evaluation, line_index) {
            CalculationValue::Integer(value) => *value,
            other => panic!("expected an integer on line {line_index}, got {other:?}"),
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

    fn formatted(evaluation: &CalculationEvaluation<'_>, line_index: usize) -> String {
        value(evaluation, line_index)
            .formatted_result()
            .expect("numeric result should have a display form")
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
    fn full_line_comments_leave_source_and_physical_references_intact() {
        let source = "  # input = missing\r\n\t// pi = 7\r\n // malformed = ( ???\r\n\r\nbase = 8 / 2\r\npi\r\nr5 * 2\r\n";
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();

        assert_eq!(evaluation.source, source);
        assert_eq!(evaluation.lines.len(), 8);
        for (index, text) in ["  # input = missing", "\t// pi = 7", " // malformed = ( ???"]
            .into_iter()
            .enumerate()
        {
            let line = &evaluation.lines[index];
            assert!(matches!(line.outcome, LineOutcome::Comment));
            assert_eq!(&evaluation.source[line.source_range.clone()], text);
            assert_eq!(line.assignment_name, None);
            assert!(line.dependency_indices.is_empty());
        }
        assert!(matches!(evaluation.lines[3].outcome, LineOutcome::Blank));
        assert_eq!(scalar(&evaluation, 4), 4.0);
        assert_eq!(scalar(&evaluation, 5), std::f64::consts::PI);
        assert_eq!(scalar(&evaluation, 6), 8.0);
        assert_eq!(evaluation.lines[6].dependency_indices, vec![4]);
        assert!(matches!(evaluation.lines[7].outcome, LineOutcome::Blank));
        assert!(!super::declared_names(source).contains("pi"));
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
    fn evaluates_balanced_nested_minimum_and_maximum_calls() {
        let document = CalculationDocument::new(
            "min(100, min(50, 75))\nmin(min(100, 50), 75)\nmax(min(100, 50), min(75, 60))\nbase = 20\nmin(base + 40, max(base * 2, 50))\nx = min(100,min(50,75)\nmin(100, min(1 / 0, 75))\nMAX(min(100, 50), mod(-7, 3))\natan2(min(2, 1), max(1, 2))\nmin(1)\nmin(1, 2, 3)",
        );
        let evaluation = document.evaluate();

        assert_eq!(scalar(&evaluation, 0), 50.0);
        assert_eq!(scalar(&evaluation, 1), 50.0);
        assert_eq!(scalar(&evaluation, 2), 60.0);
        assert_eq!(scalar(&evaluation, 4), 50.0);
        assert!(matches!(error(&evaluation, 5), DiagnosticKind::InvalidExpression));
        assert!(matches!(error(&evaluation, 6), DiagnosticKind::InvalidArithmetic));
        assert_eq!(scalar(&evaluation, 7), 50.0);
        assert_eq!(scalar(&evaluation, 8), 1.0_f64.atan2(2.0));
        assert!(matches!(error(&evaluation, 9), DiagnosticKind::Script(message) if message.contains("collection")));
        assert!(matches!(error(&evaluation, 10), DiagnosticKind::InvalidExpression));
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
        assert_eq!(scalar(&evaluation, 6), 1.0);
        assert_eq!(scalar(&evaluation, 7), 2.0);
        assert_eq!(scalar(&evaluation, 8), 3.0);
        assert_eq!(scalar(&evaluation, 9), 4.0);
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
    fn sequential_assignments_use_the_preceding_binding_and_keep_physical_rows() {
        let document = CalculationDocument::new("x = 2\nold = x\nx = x + 3\nr1\nx");
        let evaluation = document.evaluate();
        for (line, expected) in [2.0, 2.0, 5.0, 2.0, 5.0].into_iter().enumerate() {
            assert_eq!(scalar(&evaluation, line), expected);
        }
        assert_eq!(evaluation.lines[1].dependency_indices, [0]);
        assert_eq!(evaluation.lines[2].dependency_indices, [0]);
        assert_eq!(evaluation.lines[3].dependency_indices, [0]);
        assert_eq!(evaluation.lines[4].dependency_indices, [2]);
        assert_eq!(evaluation.final_bindings.get("x"), Some(&2));
        assert_eq!(evaluation.final_bindings.get("old"), Some(&1));
    }

    #[test]
    fn failed_reassignment_hides_the_previous_value_but_not_other_bindings() {
        let document = CalculationDocument::new(concat!(
            "x = 1\n",
            "x =\n",
            "x\n",
            "x = 1 = 2\n",
            "x\n",
            "x = 8\n",
            "x\n",
            "{sin} = 5\n",
            "sin = 9\n",
            "{sin}\n",
            "independent = 7",
        ));
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, 1),
            DiagnosticKind::InvalidExpression
        ));
        assert!(matches!(
            error(&evaluation, 2),
            DiagnosticKind::DependencyFailed { dependency_line: 1 }
        ));
        assert!(matches!(
            error(&evaluation, 3),
            DiagnosticKind::InvalidAssignment
        ));
        assert!(matches!(
            error(&evaluation, 4),
            DiagnosticKind::DependencyFailed { dependency_line: 3 }
        ));
        assert_eq!(scalar(&evaluation, 6), 8.0);
        assert!(matches!(
            error(&evaluation, 8),
            DiagnosticKind::ReservedName(_)
        ));
        assert_eq!(scalar(&evaluation, 9), 5.0);
        assert_eq!(scalar(&evaluation, 10), 7.0);
        assert_eq!(evaluation.final_bindings.get("x"), Some(&5));
        assert_eq!(evaluation.final_bindings.get("sin"), Some(&7));
    }

    #[test]
    fn reassignment_preserves_exact_integer_and_quantity_types() {
        let document = CalculationDocument::new(concat!(
            "{雪} = u64(9007199254740993)\n",
            "before = {雪}\n",
            "{雪} = {雪} + u64(1)\n",
            "{雪}\n",
            "q = 1 m\n",
            "q = q + 2 m\n",
            "q"
        ));
        let evaluation = document.evaluate();
        assert_eq!(integer(&evaluation, 1).bit_pattern(), 9007199254740993);
        assert_eq!(integer(&evaluation, 3).bit_pattern(), 9007199254740994);
        assert_eq!(formatted(&evaluation, 6), "3 m");
        assert_eq!(evaluation.lines[2].dependency_indices, [0]);
        assert_eq!(evaluation.lines[5].dependency_indices, [4]);
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
            DiagnosticKind::Quantity(QuantityError::IncompatibleDimensions)
        ));
        // Scalar expressions cannot be added to a dimensioned quantity.
        assert!(matches!(
            error(&evaluation, 8),
            DiagnosticKind::Quantity(QuantityError::IncompatibleDimensions)
        ));
        assert!(matches!(
            error(&evaluation, 10),
            DiagnosticKind::ExpectedPhysicalQuantity(_)
        ));
    }

    #[test]
    fn evaluates_dimensional_arithmetic_references_and_unit_targets() {
        let source = concat!(
            "sum = 10 m + 20 cm\n",
            "area = (2 m)*(3 m)\n",
            "area to cm^2\n",
            "speed = (5 m)/(2 s)\n",
            "speed to km/h\n",
            "speed * 2\n",
            "distance = 2 m\n",
            "4/distance\n",
            "4/(2 m)\n",
            "2/3^2 * (1 m)\n",
            "m = 3\n",
            "s = 2\n",
            "m / s\n",
            "distance / s\n",
            "distance / (2 s)\n",
            "{長さ} = 5 m\n",
            "{長さ} * 2\n",
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();

        assert_eq!(evaluation.source, source);
        assert_eq!(formatted(&evaluation, 0), "10.2 m");
        assert_eq!(formatted(&evaluation, 1), "6 m²");
        assert_eq!(formatted(&evaluation, 2), "60000 cm²");
        assert_eq!(formatted(&evaluation, 3), "2.5 m/s");
        assert_eq!(formatted(&evaluation, 4), "9 kph");
        assert_eq!(formatted(&evaluation, 5), "5 m/s");
        assert_eq!(formatted(&evaluation, 7), "2 1/m");
        assert_eq!(formatted(&evaluation, 8), "2 1/m");
        assert_eq!(formatted(&evaluation, 9), "0.222222 m");
        assert_eq!(scalar(&evaluation, 12), 1.5);
        assert_eq!(formatted(&evaluation, 13), "1 m");
        assert_eq!(formatted(&evaluation, 14), "1 m/s");
        assert_eq!(formatted(&evaluation, 16), "10 m");
    }

    #[test]
    fn legacy_scalar_min_coexists_with_minutes() {
        let source = concat!(
            "2 min 3\n",
            "x = 2\n",
            "y = 5\n",
            "x min y\n",
            "2 min -3\n",
            "2 min + 3\n",
            "2 min\n",
            "2 min + 3 min\n",
            "2 min + 3 m\n",
            "distance = 3 m\n",
            "2 min + distance\n",
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();

        assert_eq!(scalar(&evaluation, 0), 2.0);
        assert_eq!(scalar(&evaluation, 3), 2.0);
        assert_eq!(scalar(&evaluation, 4), -3.0);
        assert_eq!(scalar(&evaluation, 5), 2.0);
        assert_eq!(formatted(&evaluation, 6), "2 min");
        assert_eq!(formatted(&evaluation, 7), "5 min");
        assert!(matches!(
            error(&evaluation, 8),
            DiagnosticKind::Quantity(QuantityError::IncompatibleDimensions)
        ));
        assert!(matches!(
            error(&evaluation, 10),
            DiagnosticKind::Quantity(QuantityError::IncompatibleDimensions)
        ));
    }

    #[test]
    fn quantity_provenance_survives_cancellation_and_display_range_is_checked() {
        let document = CalculationDocument::new(concat!(
            "cancelled = 1 month/month\n",
            "zero_power = 1 month^0\n",
            "ratio = (1 month)/(1 month)\n",
            "2 ^ ratio\n",
            "(2 MB/s)^2\n",
            "1/(1 MB/s)\n",
            "((1e308 m)*1) to mm\n",
            "1 m + 1 s\n",
            "1 m + 1 kg\n",
            "1 K * 2\n",
            "1 m / 0\n",
            "1 m^9\n",
            "1/(1 K)\n",
            "4/(2 m)\n",
            "3 m\n",
            "1 km/l * 2\n",
            "acceleration = 1 m/s^2\n",
        ));
        let evaluation = document.evaluate();

        for index in 0..4 {
            assert!(value(&evaluation, index).is_approximate());
        }
        assert_eq!(formatted(&evaluation, 0), "1");
        assert_eq!(formatted(&evaluation, 1), "1");
        assert_eq!(formatted(&evaluation, 3), "2");
        assert_eq!(formatted(&evaluation, 4), "4 (MB/s)²");
        assert_eq!(formatted(&evaluation, 5), "1 1/(MB/s)");
        for index in 6..13 {
            assert!(matches!(
                error(&evaluation, index),
                DiagnosticKind::Quantity(_)
            ));
        }
        assert_eq!(formatted(&evaluation, 13), "2 1/m");
        assert_eq!(formatted(&evaluation, 14), "3 m");
        assert!(matches!(
            error(&evaluation, 15),
            DiagnosticKind::Quantity(QuantityError::NonLinearUnit(Unit::KilometerPerLiter))
        ));
        assert_eq!(formatted(&evaluation, 16), "1 m/s²");
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
    fn evaluates_mixed_base_scalar_literals_without_rewriting_source_or_names() {
        let source = concat!(
            "# labels 0xff\r\n",
            "value0xff = 2\r\n",
            "value0xff + 1\r\n",
            "{base 0x10} = 4\r\n",
            "{base 0x10} + 1\r\n",
            "0x1235 + 0b0101010101\r\n",
            "sqrt(0o20) + 1e2\r\n",
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();

        assert_eq!(evaluation.source, source);
        assert_eq!(scalar(&evaluation, 2), 3.0);
        assert_eq!(scalar(&evaluation, 4), 5.0);
        assert_eq!(scalar(&evaluation, 5), 5002.0);
        assert_eq!(scalar(&evaluation, 6), 104.0);
        assert_eq!(
            &evaluation.source[evaluation.lines[5].source_range.clone()],
            "0x1235 + 0b0101010101"
        );
    }

    #[test]
    fn binary_first_and_contextual_scalar_aliases_preserve_source_and_bindings() {
        let source = concat!(
            "# 0b1 Floor pie\r\n",
            "0b10101\r\n0b0101 + 123\r\n0x10 + 0o10 + 0b10\r\n",
            "sqrt(0b10000)\r\n-0B101\r\nFloor(2.9)\r\nFLOOR(2.9)\r\n",
            "floor_price = 9\r\nfloor_price + 1\r\n",
            "pi\r\npie\r\nPI\r\nPiE\r\n",
            "α = 4\r\nα + 0B10\r\n0X10 + 0O10 + 0B10\r\n",
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();
        assert_eq!(evaluation.source, source);
        for (line, expected) in [
            (1, 21.0),
            (2, 128.0),
            (3, 26.0),
            (4, 4.0),
            (5, -5.0),
            (6, 2.0),
            (7, 2.0),
            (9, 10.0),
            (15, 6.0),
            (16, 26.0),
        ] {
            assert_eq!(scalar(&evaluation, line), expected, "line {}", line + 1);
        }
        for line in 10..=13 {
            assert_eq!(scalar(&evaluation, line), std::f64::consts::PI);
        }
        assert_eq!(
            &source[evaluation.lines[2].source_range.clone()],
            "0b0101 + 123"
        );

        let document = CalculationDocument::new("pi = 7\npie = 8\npi + pie\n{pi} + 1\n");
        let evaluation = document.evaluate();
        assert_eq!(scalar(&evaluation, 2), 15.0);
        assert_eq!(scalar(&evaluation, 3), 8.0);
        let document = CalculationDocument::new("pi\npi = 7\n");
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::ForwardReference { .. }
        ));
        let document = CalculationDocument::new("pie = 1\npie = 2\npie\n");
        let evaluation = document.evaluate();
        assert_eq!(scalar(&evaluation, 2), 2.0);
    }

    #[test]
    fn infix_mod_matches_signed_remainder_and_keeps_mod_function_and_variable() {
        let document = CalculationDocument::new(concat!(
            "mod = 5\nmod + 1\n{mod} + 2\n7 mod 3\n-7 mod 3\n",
            "7 mod -3\n20 mod 6 / 2\n2 * 5 mod 3\n",
            "mod(-7, 3)\n7 mod (3)\n1 mod 0\n7 mod mod\n7 mod mod + 1\n",
        ));
        let evaluation = document.evaluate();
        for (line, expected) in [
            (1, 6.0),
            (2, 7.0),
            (3, 1.0),
            (4, -1.0),
            (5, 1.0),
            (6, 1.0),
            (7, 1.0),
            (8, 2.0),
            (9, 1.0),
        ] {
            assert_eq!(scalar(&evaluation, line), expected, "line {}", line + 1);
        }
        assert!(matches!(
            error(&evaluation, 10),
            DiagnosticKind::InvalidArithmetic
        ));
        assert_eq!(scalar(&evaluation, 11), 2.0);
        assert_eq!(scalar(&evaluation, 12), 3.0);
    }

    #[test]
    fn pi_aliases_are_resolved_in_quantities_and_comparison_contexts() {
        use crate::calculation_pad::comparison::ComparisonRow;
        let document = CalculationDocument::new("pi * 1 m\nPI * 1 m\n0b10 * 1 m\n");
        let evaluation = document.evaluate();
        assert_eq!(formatted(&evaluation, 0), formatted(&evaluation, 1));
        assert_eq!(formatted(&evaluation, 0), "3.14159 m");
        assert_eq!(formatted(&evaluation, 2), "2 m");

        let document = CalculationDocument::new("pi = 7\npi * 1 m\n");
        let evaluation = document.evaluate();
        assert_eq!(formatted(&evaluation, 1), "7 m");
        let rows = vec![ComparisonRow {
            id: crate::calculation_pad::LineId(1),
            assignment_name: Some("pi".into()),
            outcome: LineOutcome::Value(CalculationValue::Scalar(7.0)),
        }];
        let bindings = std::collections::HashMap::from([("pi".to_owned(), 0)]);
        assert_eq!(
            super::evaluate_comparison_expression("pi + 1", &rows, &bindings),
            LineOutcome::Value(CalculationValue::Scalar(8.0))
        );
        assert_eq!(
            super::evaluate_comparison_expression("pie", &rows, &bindings),
            LineOutcome::Value(CalculationValue::Scalar(std::f64::consts::PI))
        );
    }

    #[test]
    fn infix_mod_matches_typed_remainder_including_zero_error() {
        let document = CalculationDocument::new(
            "u8(7) % u8(3)\nu8(7) mod u8(3)\nu8(7) % u8(0)\nu8(7) mod u8(0)\n",
        );
        let evaluation = document.evaluate();
        assert_eq!(integer(&evaluation, 0), integer(&evaluation, 1));
        assert_eq!(error(&evaluation, 2), error(&evaluation, 3));
    }

    #[test]
    fn vector_arithmetic_products_and_dependencies_use_scalar_components() {
        let source = concat!(
            "a = [1, 2, 3]\n",
            "a * 3\n3 * a\na + [1, 1, 1]\na * [4, 5, 6]\n",
            "dot(a, [4, 5, 6])\na · [4, 5, 6]\n",
            "cross([1, 0, 0], [0, 1, 0])\n[1, 0, 0] × [0, 1, 0]\n",
            "x = 5\n[Floor(2.9), x + 1, sqrt(0b10000)]\n",
            "r1 + a\n(a * (2 + 3)) / 5\n[12] / 2 / 3\n",
            "DOT([1,2], [3,4])\n{x[0]} = 7\n[{x[0]}]\n{x[0]} + 1\n",
            "[dot([1], [2]), 3]\n[dot(a, a)]\na * (6 mod 4)\n"
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();
        for (line, expected) in [
            (0, "[1, 2, 3]"),
            (1, "[3, 6, 9]"),
            (2, "[3, 6, 9]"),
            (3, "[2, 3, 4]"),
            (4, "[4, 10, 18]"),
            (7, "[0, 0, 1]"),
            (8, "[0, 0, 1]"),
            (10, "[2, 6, 4]"),
            (11, "[2, 4, 6]"),
            (12, "[1, 2, 3]"),
            (13, "[2]"),
            (16, "[7]"),
            (18, "[2, 3]"),
            (19, "[14]"),
            (20, "[2, 4, 6]"),
        ] {
            assert_eq!(formatted(&evaluation, line), expected, "line {}", line + 1);
        }
        assert_eq!(scalar(&evaluation, 5), 32.0);
        assert_eq!(scalar(&evaluation, 6), 32.0);
        assert_eq!(scalar(&evaluation, 14), 11.0);
        assert_eq!(scalar(&evaluation, 17), 8.0);
        assert_eq!(evaluation.lines[11].dependency_indices, vec![0]);
        assert_eq!(evaluation.source, source);
    }

    #[test]
    fn vector_invalid_shapes_types_and_nonfinite_work_are_local() {
        let source = concat!(
            "[]\n[[1]]\n[1,]\n[1] + [1, 2]\n[1] + 2\n",
            "[1] / 0\n[1] / [0]\ncross([1, 2], [3, 4])\n",
            "dot([1e308], [1e308])\n[1e308] * 1e308\n",
            "u8(2)\n[1] * r11\n1 m\n[1] * r13\n7\n",
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();
        for line in 0..14 {
            if line == 10 || line == 12 {
                continue;
            }
            assert!(
                matches!(evaluation.lines[line].outcome, LineOutcome::Error(_)),
                "line {}",
                line + 1
            );
        }
        assert_eq!(scalar(&evaluation, 14), 7.0);
        let too_long = format!("[{}]", vec!["1"; 129].join(","));
        let document = CalculationDocument::new(too_long);
        assert!(matches!(
            document.evaluate().lines[0].outcome,
            LineOutcome::Error(_)
        ));
        let source = format!(
            "v = [{}]\n{}",
            vec!["1"; 128].join(","),
            vec!["v"; 10].join(" * ")
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();
        assert!(
            matches!(&evaluation.lines[0].outcome, LineOutcome::Value(CalculationValue::Vector(vector)) if vector.components().len() == 128)
        );
        assert!(matches!(evaluation.lines[1].outcome, LineOutcome::Error(_)));

        let document =
            CalculationDocument::new("v = [1, 2]\nv * 6 % 4\nv * 6 mod 4\nv * 6 min 4\n");
        let evaluation = document.evaluate();
        for line in 1..4 {
            assert!(
                matches!(evaluation.lines[line].outcome, LineOutcome::Error(_)),
                "line {}",
                line + 1
            );
        }
    }

    #[test]
    fn vector_formatted_output_is_pasteable_with_display_precision() {
        let document = CalculationDocument::new("[1/3, 1.23456789e20, -2.5]\n[1, 2, 3]");
        let evaluation = document.evaluate();
        let displayed = formatted(&evaluation, 0);
        assert_eq!(formatted(&evaluation, 1), "[1, 2, 3]");
        let reparsed = CalculationDocument::new(displayed.clone());
        let result = reparsed.evaluate();
        let CalculationValue::Vector(values) = value(&result, 0) else {
            panic!("copy must be a vector")
        };
        assert!((values.components()[0] - 1.0 / 3.0).abs() < 0.000001);
        assert!((values.components()[1] / 1.23456789e20 - 1.0).abs() < 0.00001);
        assert_eq!(values.components()[2], -2.5);
        assert_eq!(formatted(&result, 0), displayed);
    }

    #[test]
    fn decimal_integer_precision_is_bounded_without_changing_exact_or_quantity_routes() {
        let document = CalculationDocument::new(concat!(
            "9007199254740992\n9007199254740993\n9007199254740993 + 1\n",
            "[9007199254740993]\nu64(9007199254740993)\n",
            "9007199254740993 dec to hex\n1e20\n9007199254740993 m\n",
            "wide = u64(1)\nwide + 9007199254740993\n",
            "distance = 1 m\nscale = 2\ndistance * scale * 9007199254740993\n",
        ));
        let evaluation = document.evaluate();
        assert_eq!(scalar(&evaluation, 0), 9007199254740992.0);
        for line in [1, 2, 3] {
            assert!(
                matches!(
                    error(&evaluation, line),
                    DiagnosticKind::ScalarIntegerTooLarge { .. }
                ),
                "line {}",
                line + 1
            );
        }
        assert_eq!(integer(&evaluation, 4).bit_pattern(), 9007199254740993);
        assert!(matches!(
            value(&evaluation, 5),
            CalculationValue::ExactBase {
                value: 9007199254740993,
                ..
            }
        ));
        assert_eq!(scalar(&evaluation, 6), 1e20);
        assert!(matches!(
            value(&evaluation, 7),
            CalculationValue::DimensionalQuantity(_)
        ));
        assert_eq!(integer(&evaluation, 9).bit_pattern(), 9007199254740994);
        assert!(matches!(
            value(&evaluation, 12),
            CalculationValue::DimensionalQuantity(_)
        ));

        let expression = format!(
            "9007199254740993 m + {}",
            vec!["x"; MAX_DEPENDENCIES_PER_LINE + 1].join(" + ")
        );
        let document = CalculationDocument::new(expression);
        assert!(matches!(
            error(&document.evaluate(), 0),
            DiagnosticKind::TooManyIdentifiers { .. }
        ));
    }

    #[test]
    fn scientific_literals_are_normalized_only_on_the_scalar_path() {
        let source = concat!(
            "1e2\r\n",
            "1.25E-2\r\n",
            "1e308\r\n",
            "1e-308\r\n",
            "0xE2 + 1e1\r\n",
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();

        assert_eq!(evaluation.source, source);
        assert_eq!(scalar(&evaluation, 0), 100.0);
        assert_eq!(scalar(&evaluation, 1), 0.0125);
        assert_eq!(scalar(&evaluation, 2), 1e308);
        assert_eq!(scalar(&evaluation, 3), 1e-308);
        assert_eq!(scalar(&evaluation, 4), 236.0);
    }

    #[test]
    fn exact_hexadecimal_conversions_keep_their_u128_grammar_and_range() {
        let document = CalculationDocument::new(
            "0bad hex to dec\n0b00 hex to dec\n20000000000001 hex to dec\n0x20000000000001\n",
        );
        let evaluation = document.evaluate();

        assert!(matches!(
            value(&evaluation, 0),
            CalculationValue::ExactBase { value: 2989, .. }
        ));
        assert!(matches!(
            value(&evaluation, 1),
            CalculationValue::ExactBase { value: 2816, .. }
        ));
        assert!(matches!(
            value(&evaluation, 2),
            CalculationValue::ExactBase {
                value: 9_007_199_254_740_993,
                ..
            }
        ));
        assert!(matches!(
            error(&evaluation, 3),
            DiagnosticKind::PrefixedIntegerTooLarge { .. }
        ));
    }

    #[test]
    fn programmer_integer_operators_are_exact_and_select_explicit_modes() {
        let source = concat!(
            "0xFF << 8\r\n",
            "u8(255) << 1\r\n",
            "not u8(0)\r\n",
            "~u8(0)\r\n",
            "i8(-1) >> 1\r\n",
            "9007199254740993 & 18446744073709551615\r\n",
            "u8(0b1100) and 0b1010\r\n",
            "u8(1) or 2\r\n",
            "u8(7) xor 3\r\n",
            "u8(2) ^ 3\r\n",
            "u8(1) or 2 and 4\r\n",
            "u8(15) idiv 2\r\n",
            "u8(15) rem 4\r\n",
            "u8(15) % 4\r\n",
        );
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();

        assert_eq!(evaluation.source, source);
        assert_eq!(integer(&evaluation, 0).formatted_decimal(), "65280");
        assert_eq!(integer(&evaluation, 0).mode().type_name(), "u64");
        assert_eq!(integer(&evaluation, 1).formatted_decimal(), "254");
        assert_eq!(integer(&evaluation, 1).mode().type_name(), "u8");
        assert_eq!(integer(&evaluation, 2).formatted_decimal(), "255");
        assert_eq!(integer(&evaluation, 3).formatted_decimal(), "255");
        assert_eq!(integer(&evaluation, 4).formatted_decimal(), "-1");
        assert_eq!(integer(&evaluation, 4).mode().type_name(), "i8");
        assert_eq!(
            integer(&evaluation, 5).formatted_decimal(),
            "9007199254740993"
        );
        assert_eq!(integer(&evaluation, 6).formatted_decimal(), "8");
        assert_eq!(integer(&evaluation, 7).formatted_decimal(), "3");
        assert_eq!(integer(&evaluation, 8).formatted_decimal(), "4");
        assert_eq!(integer(&evaluation, 9).formatted_decimal(), "8");
        assert_eq!(integer(&evaluation, 10).formatted_decimal(), "1");
        assert_eq!(integer(&evaluation, 11).formatted_decimal(), "7");
        assert_eq!(integer(&evaluation, 12).formatted_decimal(), "3");
        assert_eq!(integer(&evaluation, 13).formatted_decimal(), "3");
    }

    #[test]
    fn programmer_helpers_and_typed_references_keep_integer_modes() {
        let document = CalculationDocument::new(concat!(
            "α = u8(41)\n",
            "α + 1\n",
            "r1 + 2\n",
            "rotl(u8(129), 1)\n",
            "rotr(u8(129), 1)\n",
            "rotl(u8(129), 255)\n",
            "rotl(u8(129), 256)\n",
            "rotr(i8(-1), 128)\n",
            "popcount(u8(0xb5))\n",
            "mask(u8(0), 2, 3)\n",
            "extract(i8(-1), 0, 8)\n",
        ));
        let evaluation = document.evaluate();

        assert_eq!(integer(&evaluation, 0).mode().type_name(), "u8");
        assert_eq!(integer(&evaluation, 1).formatted_decimal(), "42");
        assert_eq!(integer(&evaluation, 1).mode().type_name(), "u8");
        assert_eq!(integer(&evaluation, 2).formatted_decimal(), "43");
        assert_eq!(integer(&evaluation, 2).mode().type_name(), "u8");
        assert_eq!(integer(&evaluation, 3).bit_pattern(), 3);
        assert_eq!(integer(&evaluation, 4).bit_pattern(), 192);
        assert_eq!(integer(&evaluation, 5).bit_pattern(), 192);
        assert_eq!(integer(&evaluation, 6).bit_pattern(), 129);
        assert_eq!(integer(&evaluation, 7).formatted_decimal(), "-1");
        assert_eq!(integer(&evaluation, 8).formatted_decimal(), "5");
        assert_eq!(integer(&evaluation, 9).bit_pattern(), 0x1c);
        assert_eq!(integer(&evaluation, 10).formatted_decimal(), "255");
        assert_eq!(integer(&evaluation, 10).mode().type_name(), "u8");
    }

    #[test]
    fn programmer_errors_are_isolated_and_scalar_conversion_is_explicit() {
        let document = CalculationDocument::new(concat!(
            "scalar = 2.0\n",
            "scalar & 1\n",
            "int(scalar) & 1\n",
            "u8(2.5)\n",
            "u8(255) + 1\n",
            "u8(1) idiv 0\n",
            "i8(-128) idiv -1\n",
            "i8(-128) rem -1\n",
            "u8(1) << 8\n",
            "u8(1) << -1\n",
            "u8(1) << u64(8)\n",
            "u8(1) + i8(1)\n",
            "u8(1) / 2\n",
            "distance = 1 m to cm\n",
            "distance & 1\n",
            "u8(2) + 3\n",
        ));
        let evaluation = document.evaluate();

        assert!(matches!(
            error(&evaluation, 1),
            DiagnosticKind::Programmer(ProgrammerError::ExpectedInteger)
        ));
        assert_eq!(integer(&evaluation, 2).formatted_decimal(), "0");
        assert_eq!(integer(&evaluation, 2).mode().type_name(), "i64");
        assert!(matches!(
            error(&evaluation, 3),
            DiagnosticKind::Programmer(ProgrammerError::UnsafeScalarConversion)
        ));
        for line in [4, 5, 6, 7, 8, 9, 10, 11] {
            assert!(matches!(
                error(&evaluation, line),
                DiagnosticKind::Programmer(ProgrammerError::Integer(_))
            ));
        }
        assert!(matches!(
            error(&evaluation, 12),
            DiagnosticKind::Programmer(ProgrammerError::UnsupportedOperation)
        ));
        assert!(matches!(
            error(&evaluation, 14),
            DiagnosticKind::Programmer(ProgrammerError::ExpectedInteger)
        ));
        assert_eq!(integer(&evaluation, 15).formatted_decimal(), "5");
    }

    #[test]
    fn contextual_operator_constructor_and_unicode_names_remain_assignable() {
        let document = CalculationDocument::new(concat!(
            "and = 7\n",
            "and + 1\n",
            "not = 7\n",
            "not + 2\n",
            "not - 2\n",
            "u8 = 3\n",
            "u8 + 1\n",
            "{xor} = 9\n",
            "{xor} + 1\n",
            "u8({xor}) & 1\n",
            "not(u8(0))\n",
        ));
        let evaluation = document.evaluate();

        assert_eq!(scalar(&evaluation, 1), 8.0);
        assert_eq!(scalar(&evaluation, 3), 9.0);
        assert_eq!(scalar(&evaluation, 4), 5.0);
        assert_eq!(scalar(&evaluation, 6), 4.0);
        assert_eq!(scalar(&evaluation, 8), 10.0);
        assert_eq!(integer(&evaluation, 9).formatted_decimal(), "1");
        assert_eq!(integer(&evaluation, 10).formatted_decimal(), "255");
    }

    #[test]
    fn malformed_and_resource_limited_programmer_expressions_do_not_hide_later_lines() {
        let malformed = "u8(1) <<\nu8(2) + 3\n";
        let malformed_document = CalculationDocument::new(malformed);
        let evaluation = malformed_document.evaluate();
        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::Programmer(ProgrammerError::InvalidSyntax)
        ));
        assert_eq!(integer(&evaluation, 1).formatted_decimal(), "5");

        let nested = format!(
            "{}1 & 1{}",
            "(".repeat(MAX_EXPRESSION_NESTING + 1),
            ")".repeat(MAX_EXPRESSION_NESTING + 1)
        );
        let nested_document = CalculationDocument::new(nested);
        let nested_evaluation = nested_document.evaluate();
        assert!(matches!(
            error(&nested_evaluation, 0),
            DiagnosticKind::NestingTooDeep { .. }
        ));

        let too_many_tokens = format!("1 & 1{}", "+ 1".repeat(130));
        let token_document = CalculationDocument::new(too_many_tokens);
        let token_evaluation = token_document.evaluate();
        assert!(matches!(
            error(&token_evaluation, 0),
            DiagnosticKind::Programmer(ProgrammerError::TooManyTokens { .. })
        ));

        let oversized_name = format!(
            "u128(0x20000000000001) | {{{}}}",
            "x".repeat(MAX_NAME_BYTES + 1)
        );
        let oversized_document = CalculationDocument::new(oversized_name);
        let oversized_evaluation = oversized_document.evaluate();
        assert!(matches!(
            error(&oversized_evaluation, 0),
            DiagnosticKind::NameTooLong { .. }
        ));

        let malformed_literal_document = CalculationDocument::new("missing + 0xG");
        let malformed_literal = malformed_literal_document.evaluate();
        assert!(matches!(
            error(&malformed_literal, 0),
            DiagnosticKind::InvalidPrefixedInteger { radix: 16 }
        ));
    }

    #[test]
    fn typed_references_can_adopt_large_exact_literals_without_changing_scalar_errors() {
        let document = CalculationDocument::new(concat!(
            "wide = u128(9007199254740993)\n",
            "wide + 0x20000000000001\n",
            "missing + 0x20000000000001\n",
        ));
        let evaluation = document.evaluate();

        assert_eq!(
            integer(&evaluation, 1).formatted_decimal(),
            "18014398509481986"
        );
        assert_eq!(integer(&evaluation, 1).mode().type_name(), "u128");
        assert!(matches!(
            error(&evaluation, 2),
            DiagnosticKind::PrefixedIntegerTooLarge { .. }
        ));
    }

    #[test]
    fn integer_constructors_reject_rounded_float_spellings_but_convert_safe_scalar_values() {
        let document = CalculationDocument::new(concat!(
            "scalar = 42.0\n",
            "u8(scalar)\n",
            "u64(9007199254740993.0)\n",
            "u8(1.00000000000000001)\n",
        ));
        let evaluation = document.evaluate();

        assert_eq!(integer(&evaluation, 1).formatted_decimal(), "42");
        assert_eq!(integer(&evaluation, 1).mode().type_name(), "u8");
        assert!(matches!(
            error(&evaluation, 2),
            DiagnosticKind::Programmer(ProgrammerError::UnsafeScalarConversion)
        ));
        assert!(matches!(
            error(&evaluation, 3),
            DiagnosticKind::Programmer(ProgrammerError::UnsafeScalarConversion)
        ));
    }

    #[test]
    fn scientific_literals_report_malformed_and_out_of_range_exponents() {
        let document = CalculationDocument::new("1e\n1e+\n1e309\n");
        let evaluation = document.evaluate();

        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::InvalidScientificLiteral
        ));
        assert!(matches!(
            error(&evaluation, 1),
            DiagnosticKind::InvalidScientificLiteral
        ));
        assert!(matches!(
            error(&evaluation, 2),
            DiagnosticKind::ScientificLiteralOutOfRange
        ));
    }

    #[test]
    fn scientific_expansion_is_bounded_after_the_raw_expression_limit() {
        let source = std::iter::repeat_n("1e-308", 250)
            .collect::<Vec<_>>()
            .join("+");
        assert!(source.len() <= MAX_EXPRESSION_BYTES);

        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();
        assert!(matches!(
            error(&evaluation, 0),
            DiagnosticKind::PreparedExpressionTooLong { .. }
        ));
    }

    #[test]
    fn accepts_the_exact_scalar_integer_limit_and_rejects_the_next_integer() {
        let document =
            CalculationDocument::new("0x20000000000000\n0x1fffffffffffff\n0x20000000000001\n");
        let evaluation = document.evaluate();

        assert_eq!(scalar(&evaluation, 0), 9_007_199_254_740_992.0);
        assert_eq!(scalar(&evaluation, 1), 9_007_199_254_740_991.0);
        assert!(matches!(
            error(&evaluation, 2),
            DiagnosticKind::PrefixedIntegerTooLarge {
                max_exact: 9_007_199_254_740_992
            }
        ));
        assert!(error_message(&evaluation, 2).contains("exactly representable"));
    }

    #[test]
    fn diagnoses_missing_invalid_and_overflowing_prefixed_integer_digits() {
        let document =
            CalculationDocument::new("0x\n0b102\n0o8\n0x100000000000000000000000000000000\n");
        let evaluation = document.evaluate();

        for line in 0..3 {
            assert!(matches!(
                error(&evaluation, line),
                DiagnosticKind::InvalidPrefixedInteger { .. }
            ));
        }
        assert!(matches!(
            error(&evaluation, 3),
            DiagnosticKind::PrefixedIntegerOverflow { radix: 16 }
        ));
    }

    #[test]
    fn prefixed_literals_do_not_consume_identifier_limits() {
        let source = std::iter::repeat_n("0x1", MAX_DEPENDENCIES_PER_LINE + 1)
            .collect::<Vec<_>>()
            .join(" + ");
        let document = CalculationDocument::new(source);
        let evaluation = document.evaluate();

        assert_eq!(
            scalar(&evaluation, 0),
            (MAX_DEPENDENCIES_PER_LINE + 1) as f64
        );
    }

    #[test]
    fn supports_remainder_and_contextual_euclidean_mod_without_reserving_mod() {
        let document = CalculationDocument::new(
            "mod = 5\nmod + 1\n{mod} + 2\nmod(-7, 3)\nmod(7, -3)\nmod(mod(-7, 3), 2)\n-7 % 3\n20 % 6 / 2\n2 * 5 % 3\n20 / 6 % 2\nmax(mod(-7, 3), 1)\n",
        );
        let evaluation = document.evaluate();

        assert_eq!(scalar(&evaluation, 1), 6.0);
        assert_eq!(scalar(&evaluation, 2), 7.0);
        assert_eq!(scalar(&evaluation, 3), 2.0);
        assert_eq!(scalar(&evaluation, 4), 1.0);
        assert_eq!(scalar(&evaluation, 5), 0.0);
        assert_eq!(scalar(&evaluation, 6), -1.0);
        assert_eq!(scalar(&evaluation, 7), 1.0);
        assert_eq!(scalar(&evaluation, 8), 1.0);
        assert!((scalar(&evaluation, 9) - 1.3333333333333335).abs() < f64::EPSILON);
        assert_eq!(scalar(&evaluation, 10), 2.0);
    }

    #[test]
    fn remainder_rejects_zero_and_non_finite_operands() {
        let document = CalculationDocument::new("1 % 0\n(1 / 0) % 2\n1 % (1 / 0)\n");
        let evaluation = document.evaluate();

        for line in 0..3 {
            assert!(matches!(
                error(&evaluation, line),
                DiagnosticKind::InvalidArithmetic
            ));
        }
        assert!(error_message(&evaluation, 0).contains("remainder by zero"));
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
