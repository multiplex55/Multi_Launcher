//! Bounded exact-integer expression parsing and evaluation for Calculation Pad.

use super::document::{CalculationValue, MAX_NAME_BYTES};
use super::integer::{IntegerError, IntegerMode, IntegerWidth, TypedInteger};
use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

pub(crate) const MAX_PROGRAMMER_TOKENS: usize = 256;
const MAX_PARSER_RECURSION: usize = 32;
const DEFAULT_INTEGER_MODE: IntegerMode = IntegerMode::unsigned(IntegerWidth::Bits64);
const MAX_EXACT_SCALAR_INTEGER: f64 = 9_007_199_254_740_992.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgrammerError {
    InvalidSyntax,
    TooManyTokens { max_tokens: usize },
    NestingTooDeep { max_depth: usize },
    NameTooLong { max_bytes: usize },
    ExpectedInteger,
    UnsafeScalarConversion,
    UnsupportedOperation,
    UnknownFunction(String),
    InvalidFunctionArguments(String),
    Integer(IntegerError),
}

impl fmt::Display for ProgrammerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSyntax => formatter.write_str("invalid programmer expression"),
            Self::TooManyTokens { max_tokens } => write!(
                formatter,
                "programmer expression exceeds the {max_tokens}-token limit"
            ),
            Self::NestingTooDeep { max_depth } => write!(
                formatter,
                "programmer expression nesting exceeds {max_depth} levels"
            ),
            Self::NameTooLong { max_bytes } => {
                write!(formatter, "name exceeds the {max_bytes}-byte limit")
            }
            Self::ExpectedInteger => formatter.write_str(
                "programmer operations require exact integer values; convert scalar values explicitly",
            ),
            Self::UnsafeScalarConversion => formatter.write_str(
                "scalar conversion requires a finite integral value within the exact-integer range",
            ),
            Self::UnsupportedOperation => {
                formatter.write_str("operation is not supported for exact integers")
            }
            Self::UnknownFunction(name) => write!(formatter, "unknown programmer function: {name}"),
            Self::InvalidFunctionArguments(name) => {
                write!(formatter, "invalid arguments for programmer function: {name}")
            }
            Self::Integer(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ProgrammerError {}

#[derive(Debug, Clone)]
pub(crate) struct ProgrammerExpression {
    root: Expr,
}

impl ProgrammerExpression {
    pub(crate) fn references(&self) -> Vec<(String, bool)> {
        let mut references = Vec::new();
        self.root.collect_references(&mut references);
        references
    }

    pub(crate) fn evaluate(
        &self,
        references: &HashMap<String, CalculationValue>,
    ) -> Result<TypedInteger, ProgrammerError> {
        let mode = infer_mode(&self.root, references)?.unwrap_or(DEFAULT_INTEGER_MODE);
        evaluate_as(&self.root, mode, references)
    }
}

#[derive(Debug)]
pub(crate) struct ParseOutcome {
    pub(crate) expression: Option<ProgrammerExpression>,
    pub(crate) error: Option<ProgrammerError>,
    pub(crate) uses_programmer_syntax: bool,
    pub(crate) identifier_count: usize,
}

pub(crate) fn parse(source: &str) -> ParseOutcome {
    let tokenized = tokenize(source);
    let mut uses_programmer_syntax = tokenized.uses_programmer_syntax;
    let identifier_count = tokenized.identifier_count;
    if let Some(error) = tokenized.error {
        return ParseOutcome {
            expression: None,
            error: Some(error),
            uses_programmer_syntax,
            identifier_count,
        };
    }

    let mut parser = Parser::new(tokenized.tokens);
    let parsed = parser.parse();
    uses_programmer_syntax |= parser.uses_programmer_syntax;
    match parsed {
        Ok(root) => ParseOutcome {
            expression: Some(ProgrammerExpression { root }),
            error: None,
            uses_programmer_syntax,
            identifier_count,
        },
        Err(error) => ParseOutcome {
            expression: None,
            error: Some(error),
            uses_programmer_syntax,
            identifier_count,
        },
    }
}

#[derive(Debug, Clone)]
struct Expr {
    kind: ExprKind,
    span: Range<usize>,
}

#[derive(Debug, Clone)]
enum ExprKind {
    Number(String),
    Name {
        name: String,
        braced: bool,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        name: String,
        braced: bool,
        arguments: Vec<Expr>,
    },
}

impl Expr {
    fn collect_references(&self, references: &mut Vec<(String, bool)>) {
        match &self.kind {
            ExprKind::Number(_) => {}
            ExprKind::Name { name, braced } => references.push((name.clone(), *braced)),
            ExprKind::Unary { operand, .. } => operand.collect_references(references),
            ExprKind::Binary { left, right, .. } => {
                left.collect_references(references);
                right.collect_references(references);
            }
            ExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    argument.collect_references(references);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnaryOp {
    Plus,
    Negate,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Power,
    IntegerDivide,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

#[derive(Debug, Clone, Copy)]
enum IntegerCount {
    Signed(i128),
    Unsigned(u128),
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    span: Range<usize>,
}

#[derive(Debug, Clone)]
enum TokenKind {
    Number(String),
    Identifier { name: String, braced: bool },
    Symbol(Symbol),
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Symbol {
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Ampersand,
    Pipe,
    Tilde,
    ShiftLeft,
    ShiftRight,
    LeftParen,
    RightParen,
    Comma,
}

struct Tokenized {
    tokens: Vec<Token>,
    error: Option<ProgrammerError>,
    uses_programmer_syntax: bool,
    identifier_count: usize,
}

fn tokenize(source: &str) -> Tokenized {
    let mut tokens = Vec::new();
    let mut cursor = 0;
    let mut identifier_count = 0;
    let mut uses_programmer_syntax = false;

    while cursor < source.len() {
        let character = source[cursor..]
            .chars()
            .next()
            .expect("cursor is in bounds");
        if character.is_whitespace() {
            cursor += character.len_utf8();
            continue;
        }
        if tokens.len() >= MAX_PROGRAMMER_TOKENS {
            return Tokenized {
                tokens,
                error: Some(ProgrammerError::TooManyTokens {
                    max_tokens: MAX_PROGRAMMER_TOKENS,
                }),
                uses_programmer_syntax,
                identifier_count,
            };
        }

        let start = cursor;
        let kind = match character {
            '{' => {
                cursor += 1;
                let name_start = cursor;
                let mut found_close = false;
                while cursor < source.len() {
                    let next = source[cursor..]
                        .chars()
                        .next()
                        .expect("cursor is in bounds");
                    if next == '}' {
                        found_close = true;
                        break;
                    }
                    if matches!(next, '{' | '\n' | '\r') {
                        break;
                    }
                    cursor += next.len_utf8();
                }
                if !found_close || cursor == name_start {
                    return Tokenized {
                        tokens,
                        error: Some(ProgrammerError::InvalidSyntax),
                        uses_programmer_syntax,
                        identifier_count,
                    };
                }
                let name = source[name_start..cursor].to_owned();
                cursor += 1;
                if name.len() > MAX_NAME_BYTES {
                    return Tokenized {
                        tokens,
                        error: Some(ProgrammerError::NameTooLong {
                            max_bytes: MAX_NAME_BYTES,
                        }),
                        uses_programmer_syntax,
                        identifier_count,
                    };
                }
                identifier_count += 1;
                TokenKind::Identifier { name, braced: true }
            }
            '0'..='9' | '.' if character.is_ascii() => {
                cursor = scan_number(source, cursor);
                TokenKind::Number(source[start..cursor].to_owned())
            }
            character if is_identifier_start(character) => {
                cursor += character.len_utf8();
                while cursor < source.len() {
                    let next = source[cursor..]
                        .chars()
                        .next()
                        .expect("cursor is in bounds");
                    if !is_identifier_continue(next) {
                        break;
                    }
                    cursor += next.len_utf8();
                }
                let name = source[start..cursor].to_owned();
                if name.len() > MAX_NAME_BYTES {
                    return Tokenized {
                        tokens,
                        error: Some(ProgrammerError::NameTooLong {
                            max_bytes: MAX_NAME_BYTES,
                        }),
                        uses_programmer_syntax,
                        identifier_count,
                    };
                }
                identifier_count += 1;
                TokenKind::Identifier {
                    name,
                    braced: false,
                }
            }
            '<' if source[start..].starts_with("<<") => {
                cursor += 2;
                uses_programmer_syntax = true;
                TokenKind::Symbol(Symbol::ShiftLeft)
            }
            '>' if source[start..].starts_with(">>") => {
                cursor += 2;
                uses_programmer_syntax = true;
                TokenKind::Symbol(Symbol::ShiftRight)
            }
            '<' | '>' => {
                return Tokenized {
                    tokens,
                    error: Some(ProgrammerError::InvalidSyntax),
                    uses_programmer_syntax,
                    identifier_count,
                };
            }
            '+' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::Plus)
            }
            '-' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::Minus)
            }
            '*' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::Star)
            }
            '/' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::Slash)
            }
            '%' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::Percent)
            }
            '^' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::Caret)
            }
            '&' => {
                cursor += 1;
                uses_programmer_syntax = true;
                TokenKind::Symbol(Symbol::Ampersand)
            }
            '|' => {
                cursor += 1;
                uses_programmer_syntax = true;
                TokenKind::Symbol(Symbol::Pipe)
            }
            '~' => {
                cursor += 1;
                uses_programmer_syntax = true;
                TokenKind::Symbol(Symbol::Tilde)
            }
            '(' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::LeftParen)
            }
            ')' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::RightParen)
            }
            ',' => {
                cursor += 1;
                TokenKind::Symbol(Symbol::Comma)
            }
            _ => {
                return Tokenized {
                    tokens,
                    error: Some(ProgrammerError::InvalidSyntax),
                    uses_programmer_syntax,
                    identifier_count,
                };
            }
        };
        tokens.push(Token {
            kind,
            span: start..cursor,
        });
    }

    tokens.push(Token {
        kind: TokenKind::End,
        span: source.len()..source.len(),
    });
    Tokenized {
        tokens,
        error: None,
        uses_programmer_syntax,
        identifier_count,
    }
}

fn scan_number(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    if bytes[start] == b'0'
        && let Some(prefix) = bytes.get(start + 1).copied()
        && matches!(prefix.to_ascii_lowercase(), b'x' | b'b' | b'o')
    {
        let mut cursor = start + 2;
        while bytes.get(cursor).is_some_and(u8::is_ascii_alphanumeric) {
            cursor += 1;
        }
        return cursor;
    }

    let mut cursor = start;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
    }
    if bytes
        .get(cursor)
        .is_some_and(|byte| matches!(*byte, b'e' | b'E'))
    {
        let exponent_start = cursor;
        cursor += 1;
        if bytes
            .get(cursor)
            .is_some_and(|byte| matches!(*byte, b'+' | b'-'))
        {
            cursor += 1;
        }
        let digits_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == digits_start {
            cursor = exponent_start + 1;
        }
    }
    cursor
}

fn is_identifier_start(character: char) -> bool {
    character == '_'
        || character.is_ascii_alphabetic()
        || matches!(character, 'α'..='ω' | 'Α'..='Ω')
}

fn is_identifier_continue(character: char) -> bool {
    is_identifier_start(character) || character.is_ascii_digit()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    recursion_depth: usize,
    uses_programmer_syntax: bool,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            cursor: 0,
            recursion_depth: 0,
            uses_programmer_syntax: false,
        }
    }

    fn parse(&mut self) -> Result<Expr, ProgrammerError> {
        let expression = self.parse_expression(0)?;
        if !matches!(&self.peek().kind, TokenKind::End) {
            return Err(ProgrammerError::InvalidSyntax);
        }
        Ok(expression)
    }

    fn parse_expression(&mut self, minimum_binding_power: u8) -> Result<Expr, ProgrammerError> {
        self.recursion_depth += 1;
        if self.recursion_depth > MAX_PARSER_RECURSION {
            self.recursion_depth -= 1;
            return Err(ProgrammerError::NestingTooDeep {
                max_depth: MAX_PARSER_RECURSION,
            });
        }
        let result = self.parse_expression_inner(minimum_binding_power);
        self.recursion_depth -= 1;
        result
    }

    fn parse_expression_inner(
        &mut self,
        minimum_binding_power: u8,
    ) -> Result<Expr, ProgrammerError> {
        let mut left = self.parse_prefix()?;
        while let Some((operator, left_binding_power, right_binding_power)) = self.infix_operator()
        {
            if left_binding_power < minimum_binding_power {
                break;
            }
            self.cursor += 1;
            if matches!(
                operator,
                BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor
                    | BinaryOp::IntegerDivide
                    | BinaryOp::Remainder
            ) && matches!(
                &self.tokens[self.cursor - 1].kind,
                TokenKind::Identifier { .. }
            ) {
                self.uses_programmer_syntax = true;
            }
            if matches!(
                operator,
                BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::IntegerDivide
            ) && matches!(&self.tokens[self.cursor - 1].kind, TokenKind::Symbol(_))
            {
                self.uses_programmer_syntax = true;
            }
            let right = self.parse_expression(right_binding_power)?;
            let span = left.span.start..right.span.end;
            left = Expr {
                kind: ExprKind::Binary {
                    op: operator,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ProgrammerError> {
        let token = self.next().clone();
        match token.kind {
            TokenKind::Number(number) => Ok(Expr {
                kind: ExprKind::Number(number),
                span: token.span,
            }),
            TokenKind::Identifier { name, braced } => {
                if !braced && matches!(&self.peek().kind, TokenKind::Symbol(Symbol::LeftParen)) {
                    self.parse_call(name, token.span.start)
                } else if !braced && name == "not" && self.can_start_word_not_operand() {
                    self.uses_programmer_syntax = true;
                    let operand = self.parse_expression(7)?;
                    Ok(Expr {
                        span: token.span.start..operand.span.end,
                        kind: ExprKind::Unary {
                            op: UnaryOp::Not,
                            operand: Box::new(operand),
                        },
                    })
                } else {
                    Ok(Expr {
                        kind: ExprKind::Name { name, braced },
                        span: token.span,
                    })
                }
            }
            TokenKind::Symbol(symbol @ (Symbol::Plus | Symbol::Minus | Symbol::Tilde)) => {
                if symbol == Symbol::Tilde {
                    self.uses_programmer_syntax = true;
                }
                let operand = self.parse_expression(7)?;
                let op = match symbol {
                    Symbol::Plus => UnaryOp::Plus,
                    Symbol::Minus => UnaryOp::Negate,
                    Symbol::Tilde => UnaryOp::Not,
                    _ => unreachable!(),
                };
                Ok(Expr {
                    span: token.span.start..operand.span.end,
                    kind: ExprKind::Unary {
                        op,
                        operand: Box::new(operand),
                    },
                })
            }
            TokenKind::Symbol(Symbol::LeftParen) => {
                let expression = self.parse_expression(0)?;
                let close = self.expect_symbol(Symbol::RightParen)?;
                Ok(Expr {
                    span: token.span.start..close.span.end,
                    kind: expression.kind,
                })
            }
            _ => Err(ProgrammerError::InvalidSyntax),
        }
    }

    fn parse_call(&mut self, name: String, start: usize) -> Result<Expr, ProgrammerError> {
        if is_programmer_call(&name) || name == "not" {
            self.uses_programmer_syntax = true;
        }
        self.expect_symbol(Symbol::LeftParen)?;
        let mut arguments = Vec::new();
        if !matches!(&self.peek().kind, TokenKind::Symbol(Symbol::RightParen)) {
            loop {
                arguments.push(self.parse_expression(0)?);
                if !matches!(&self.peek().kind, TokenKind::Symbol(Symbol::Comma)) {
                    break;
                }
                self.cursor += 1;
            }
        }
        let close = self.expect_symbol(Symbol::RightParen)?;
        if name == "not" && arguments.len() == 1 {
            let Some(operand) = arguments.pop() else {
                return Err(ProgrammerError::InvalidFunctionArguments(name));
            };
            return Ok(Expr {
                span: start..close.span.end,
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                },
            });
        } else if name == "not" {
            return Err(ProgrammerError::InvalidFunctionArguments(name));
        }
        Ok(Expr {
            kind: ExprKind::Call {
                name,
                braced: false,
                arguments,
            },
            span: start..close.span.end,
        })
    }

    fn infix_operator(&self) -> Option<(BinaryOp, u8, u8)> {
        let (operator, precedence, right_associative) = match &self.peek().kind {
            TokenKind::Symbol(Symbol::Pipe) => (BinaryOp::BitOr, 1, false),
            TokenKind::Symbol(Symbol::Ampersand) => (BinaryOp::BitAnd, 3, false),
            TokenKind::Symbol(Symbol::ShiftLeft) => (BinaryOp::ShiftLeft, 4, false),
            TokenKind::Symbol(Symbol::ShiftRight) => (BinaryOp::ShiftRight, 4, false),
            TokenKind::Symbol(Symbol::Plus) => (BinaryOp::Add, 5, false),
            TokenKind::Symbol(Symbol::Minus) => (BinaryOp::Subtract, 5, false),
            TokenKind::Symbol(Symbol::Star) => (BinaryOp::Multiply, 6, false),
            TokenKind::Symbol(Symbol::Slash) => (BinaryOp::Divide, 6, false),
            TokenKind::Symbol(Symbol::Percent) => (BinaryOp::Remainder, 6, false),
            TokenKind::Symbol(Symbol::Caret) => (BinaryOp::Power, 7, true),
            TokenKind::Identifier {
                name,
                braced: false,
            } => match name.as_str() {
                "or" => (BinaryOp::BitOr, 1, false),
                "xor" => (BinaryOp::BitXor, 2, false),
                "and" => (BinaryOp::BitAnd, 3, false),
                "idiv" => (BinaryOp::IntegerDivide, 6, false),
                "rem" => (BinaryOp::Remainder, 6, false),
                _ => return None,
            },
            _ => return None,
        };
        let left_binding_power = precedence;
        let right_binding_power = if right_associative {
            precedence
        } else {
            precedence + 1
        };
        Some((operator, left_binding_power, right_binding_power))
    }

    fn can_start_prefix(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Number(_)
                | TokenKind::Identifier { .. }
                | TokenKind::Symbol(
                    Symbol::LeftParen | Symbol::Plus | Symbol::Minus | Symbol::Tilde
                )
        )
    }

    fn can_start_word_not_operand(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Number(_)
                | TokenKind::Identifier { .. }
                | TokenKind::Symbol(Symbol::LeftParen | Symbol::Tilde)
        )
    }

    fn expect_symbol(&mut self, symbol: Symbol) -> Result<Token, ProgrammerError> {
        if matches!(&self.peek().kind, TokenKind::Symbol(found) if *found == symbol) {
            Ok(self.next().clone())
        } else {
            Err(ProgrammerError::InvalidSyntax)
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn next(&mut self) -> &Token {
        let token = &self.tokens[self.cursor];
        if !matches!(&token.kind, TokenKind::End) {
            self.cursor += 1;
        }
        token
    }
}

fn is_programmer_call(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "rotl"
            | "rotr"
            | "popcount"
            | "mask"
            | "extract"
    )
}

fn constructor_mode(name: &str) -> Option<IntegerMode> {
    let (signed, width) = match name {
        "int" | "i64" => (true, IntegerWidth::Bits64),
        "i8" => (true, IntegerWidth::Bits8),
        "i16" => (true, IntegerWidth::Bits16),
        "i32" => (true, IntegerWidth::Bits32),
        "i128" => (true, IntegerWidth::Bits128),
        "u8" => (false, IntegerWidth::Bits8),
        "u16" => (false, IntegerWidth::Bits16),
        "u32" => (false, IntegerWidth::Bits32),
        "u64" => (false, IntegerWidth::Bits64),
        "u128" => (false, IntegerWidth::Bits128),
        _ => return None,
    };
    Some(if signed {
        IntegerMode::signed(width)
    } else {
        IntegerMode::unsigned(width)
    })
}

fn infer_mode(
    expression: &Expr,
    references: &HashMap<String, CalculationValue>,
) -> Result<Option<IntegerMode>, ProgrammerError> {
    match &expression.kind {
        ExprKind::Number(_) => Ok(None),
        ExprKind::Name { name, .. } => Ok(value_mode(references.get(name))),
        ExprKind::Unary { operand, .. } => infer_mode(operand, references),
        ExprKind::Binary {
            op: BinaryOp::ShiftLeft | BinaryOp::ShiftRight,
            left,
            ..
        } => infer_mode(left, references),
        ExprKind::Binary { left, right, .. } => merge_modes(
            infer_mode(left, references)?,
            infer_mode(right, references)?,
        ),
        ExprKind::Call {
            name, arguments, ..
        } => {
            if let Some(mode) = constructor_mode(name) {
                return Ok(Some(mode));
            }
            if matches!(
                name.as_str(),
                "rotl" | "rotr" | "popcount" | "mask" | "extract"
            ) && let Some(first) = arguments.first()
            {
                let mode = infer_mode(first, references)?;
                if name == "extract" {
                    return Ok(mode.map(|mode| IntegerMode::unsigned(mode.width())));
                }
                return Ok(mode);
            }
            Ok(None)
        }
    }
}

fn merge_modes(
    left: Option<IntegerMode>,
    right: Option<IntegerMode>,
) -> Result<Option<IntegerMode>, ProgrammerError> {
    match (left, right) {
        (Some(left), Some(right)) if left != right => {
            Err(ProgrammerError::Integer(IntegerError::ModeMismatch))
        }
        (Some(mode), _) | (_, Some(mode)) => Ok(Some(mode)),
        (None, None) => Ok(None),
    }
}

fn value_mode(value: Option<&CalculationValue>) -> Option<IntegerMode> {
    match value {
        Some(CalculationValue::Integer(value)) => Some(value.mode()),
        Some(CalculationValue::ExactBase { .. }) => {
            Some(IntegerMode::unsigned(IntegerWidth::Bits128))
        }
        _ => None,
    }
}

fn evaluate_as(
    expression: &Expr,
    expected_mode: IntegerMode,
    references: &HashMap<String, CalculationValue>,
) -> Result<TypedInteger, ProgrammerError> {
    match &expression.kind {
        ExprKind::Number(number) => {
            TypedInteger::parse(number, expected_mode).map_err(ProgrammerError::Integer)
        }
        ExprKind::Name { name, .. } => match references.get(name) {
            Some(CalculationValue::Integer(value)) if value.mode() == expected_mode => Ok(*value),
            Some(CalculationValue::Integer(_)) => {
                Err(ProgrammerError::Integer(IntegerError::ModeMismatch))
            }
            Some(CalculationValue::ExactBase { value, .. }) => {
                let value = TypedInteger::from_unsigned(
                    *value,
                    IntegerMode::unsigned(IntegerWidth::Bits128),
                )
                .map_err(ProgrammerError::Integer)?;
                if value.mode() == expected_mode {
                    Ok(value)
                } else {
                    Err(ProgrammerError::Integer(IntegerError::ModeMismatch))
                }
            }
            Some(CalculationValue::Scalar(_))
            | Some(CalculationValue::Boolean(_))
            | Some(CalculationValue::Sequence(_))
            | Some(CalculationValue::Vector(_))
            | Some(CalculationValue::PhysicalQuantity { .. })
            | Some(CalculationValue::DimensionalQuantity(_)) => {
                Err(ProgrammerError::ExpectedInteger)
            }
            None if scalar_constant(name).is_some() => Err(ProgrammerError::ExpectedInteger),
            None => Err(ProgrammerError::InvalidSyntax),
        },
        ExprKind::Unary { op, operand } => {
            if *op == UnaryOp::Negate
                && let ExprKind::Number(number) = &operand.kind
            {
                return TypedInteger::parse(&format!("-{number}"), expected_mode)
                    .map_err(ProgrammerError::Integer);
            }
            let value = evaluate_as(operand, expected_mode, references)?;
            match op {
                UnaryOp::Plus => Ok(value),
                UnaryOp::Negate => TypedInteger::zero(expected_mode)
                    .checked_sub(value)
                    .map_err(ProgrammerError::Integer),
                UnaryOp::Not => Ok(value.bit_not()),
            }
        }
        ExprKind::Binary { op, left, right } => {
            let operation_mode = if matches!(*op, BinaryOp::ShiftLeft | BinaryOp::ShiftRight) {
                infer_mode(left, references)?
            } else {
                merge_modes(
                    infer_mode(left, references)?,
                    infer_mode(right, references)?,
                )?
            };
            let mode = merge_modes(Some(expected_mode), operation_mode)?.unwrap_or(expected_mode);
            let left = evaluate_as(left, mode, references)?;
            if *op == BinaryOp::ShiftLeft {
                let count = evaluate_count(right, references)?;
                return left
                    .shift_left(shift_count_argument(count)?)
                    .map_err(ProgrammerError::Integer);
            }
            if *op == BinaryOp::ShiftRight {
                let count = evaluate_count(right, references)?;
                return left
                    .shift_right(shift_count_argument(count)?)
                    .map_err(ProgrammerError::Integer);
            }
            let right = evaluate_as(right, mode, references)?;
            let result = match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Subtract => left.checked_sub(right),
                BinaryOp::Multiply => left.checked_mul(right),
                BinaryOp::Divide => return Err(ProgrammerError::UnsupportedOperation),
                BinaryOp::Remainder => left.checked_rem(right),
                BinaryOp::Power => left.checked_pow(right),
                BinaryOp::IntegerDivide => left.checked_div(right),
                BinaryOp::BitAnd => left.bit_and(right),
                BinaryOp::BitOr => left.bit_or(right),
                BinaryOp::BitXor => left.bit_xor(right),
                BinaryOp::ShiftLeft | BinaryOp::ShiftRight => unreachable!(),
            };
            result.map_err(ProgrammerError::Integer)
        }
        ExprKind::Call {
            name,
            braced,
            arguments,
        } => {
            if *braced {
                return Err(ProgrammerError::UnknownFunction(name.clone()));
            }
            if let Some(target_mode) = constructor_mode(name) {
                return evaluate_constructor(name, target_mode, arguments, references);
            }
            evaluate_helper(name, arguments, expected_mode, references)
        }
    }
}

fn evaluate_constructor(
    name: &str,
    target_mode: IntegerMode,
    arguments: &[Expr],
    references: &HashMap<String, CalculationValue>,
) -> Result<TypedInteger, ProgrammerError> {
    if arguments.len() != 1 {
        return Err(ProgrammerError::InvalidFunctionArguments(name.to_owned()));
    }
    let argument = &arguments[0];
    if let ExprKind::Number(number) = &argument.kind {
        return parse_constructor_number(number, target_mode);
    }
    if let ExprKind::Unary {
        op: UnaryOp::Negate,
        operand,
    } = &argument.kind
        && let ExprKind::Number(number) = &operand.kind
    {
        return parse_constructor_number(&format!("-{number}"), target_mode);
    }
    if let ExprKind::Name {
        name: reference, ..
    } = &argument.kind
    {
        match references.get(reference) {
            Some(CalculationValue::Integer(value)) => {
                return value
                    .checked_cast(target_mode)
                    .map_err(ProgrammerError::Integer);
            }
            Some(CalculationValue::ExactBase { value, .. }) => {
                let value = TypedInteger::from_unsigned(
                    *value,
                    IntegerMode::unsigned(IntegerWidth::Bits128),
                )
                .map_err(ProgrammerError::Integer)?;
                return value
                    .checked_cast(target_mode)
                    .map_err(ProgrammerError::Integer);
            }
            Some(CalculationValue::Scalar(value)) => return scalar_to_integer(*value, target_mode),
            Some(CalculationValue::PhysicalQuantity { .. })
            | Some(CalculationValue::Boolean(_))
            | Some(CalculationValue::Sequence(_))
            | Some(CalculationValue::Vector(_))
            | Some(CalculationValue::DimensionalQuantity(_)) => {
                return Err(ProgrammerError::ExpectedInteger);
            }
            None => {
                if let Some(value) = scalar_constant(reference) {
                    return scalar_to_integer(value, target_mode);
                }
                return Err(ProgrammerError::InvalidSyntax);
            }
        }
    }
    let argument_mode = infer_mode(argument, references)?.unwrap_or(target_mode);
    let value = evaluate_as(argument, argument_mode, references)?;
    value
        .checked_cast(target_mode)
        .map_err(ProgrammerError::Integer)
}

fn parse_constructor_number(
    number: &str,
    target_mode: IntegerMode,
) -> Result<TypedInteger, ProgrammerError> {
    match TypedInteger::parse(number, target_mode) {
        Ok(value) => Ok(value),
        Err(IntegerError::InvalidLiteral) if is_scalar_literal(number) => {
            Err(ProgrammerError::UnsafeScalarConversion)
        }
        Err(error) => Err(ProgrammerError::Integer(error)),
    }
}

fn is_scalar_literal(number: &str) -> bool {
    number.contains('.') || number.contains('e') || number.contains('E')
}

fn scalar_to_integer(value: f64, mode: IntegerMode) -> Result<TypedInteger, ProgrammerError> {
    if !value.is_finite() || value.fract() != 0.0 || value.abs() > MAX_EXACT_SCALAR_INTEGER {
        return Err(ProgrammerError::UnsafeScalarConversion);
    }
    if mode.is_signed() {
        TypedInteger::from_signed(value as i128, mode).map_err(ProgrammerError::Integer)
    } else if value < 0.0 {
        Err(ProgrammerError::Integer(IntegerError::NegativeUnsigned))
    } else {
        TypedInteger::from_unsigned(value as u128, mode).map_err(ProgrammerError::Integer)
    }
}

fn scalar_constant(name: &str) -> Option<f64> {
    match name {
        "PI" | "π" => Some(std::f64::consts::PI),
        "E" | "e" => Some(std::f64::consts::E),
        "TAU" | "τ" => Some(std::f64::consts::TAU),
        _ => None,
    }
}

fn evaluate_helper(
    name: &str,
    arguments: &[Expr],
    expected_mode: IntegerMode,
    references: &HashMap<String, CalculationValue>,
) -> Result<TypedInteger, ProgrammerError> {
    if matches!(name, "rotl" | "rotr" | "popcount") {
        if arguments.len() != if name == "popcount" { 1 } else { 2 } {
            return Err(ProgrammerError::InvalidFunctionArguments(name.to_owned()));
        }
        let mode = infer_mode(&arguments[0], references)?.unwrap_or(expected_mode);
        let value = evaluate_as(&arguments[0], mode, references)?;
        return match name {
            "popcount" => TypedInteger::from_unsigned(u128::from(value.popcount()), mode)
                .map_err(ProgrammerError::Integer),
            "rotl" | "rotr" => {
                let count = evaluate_count(&arguments[1], references)?;
                let count = rotate_count_argument(count, mode)?;
                if name == "rotl" {
                    value.rotate_left(count).map_err(ProgrammerError::Integer)
                } else {
                    value.rotate_right(count).map_err(ProgrammerError::Integer)
                }
            }
            _ => unreachable!(),
        };
    }

    if matches!(name, "mask" | "extract") {
        if arguments.len() != 3 {
            return Err(ProgrammerError::InvalidFunctionArguments(name.to_owned()));
        }
        let mode = infer_mode(&arguments[0], references)?.unwrap_or(expected_mode);
        let value = evaluate_as(&arguments[0], mode, references)?;
        let start = nonnegative_count(evaluate_count(&arguments[1], references)?)?;
        let count = nonnegative_count(evaluate_count(&arguments[2], references)?)?;
        let (Ok(start), Ok(count)) = (u32::try_from(start), u32::try_from(count)) else {
            return Err(ProgrammerError::Integer(IntegerError::OutOfRange));
        };
        return if name == "mask" {
            value
                .field_mask(start, count)
                .map_err(ProgrammerError::Integer)
        } else {
            value
                .extract_bits(start, count)
                .map_err(ProgrammerError::Integer)
        };
    }

    Err(ProgrammerError::UnknownFunction(name.to_owned()))
}

fn evaluate_count(
    expression: &Expr,
    references: &HashMap<String, CalculationValue>,
) -> Result<IntegerCount, ProgrammerError> {
    if let ExprKind::Number(number) = &expression.kind {
        let value = TypedInteger::parse(number, IntegerMode::unsigned(IntegerWidth::Bits128))
            .map_err(ProgrammerError::Integer)?;
        return Ok(IntegerCount::Unsigned(value.bit_pattern()));
    }
    if let ExprKind::Unary {
        op: UnaryOp::Negate,
        operand,
    } = &expression.kind
        && let ExprKind::Number(number) = &operand.kind
    {
        let value = TypedInteger::parse(
            &format!("-{number}"),
            IntegerMode::signed(IntegerWidth::Bits128),
        )
        .map_err(ProgrammerError::Integer)?;
        return Ok(IntegerCount::Signed(
            value
                .as_i128()
                .ok_or(ProgrammerError::Integer(IntegerError::OutOfRange))?,
        ));
    }

    let mode = infer_mode(expression, references)?.unwrap_or(DEFAULT_INTEGER_MODE);
    let value = evaluate_as(expression, mode, references)?;
    if let Some(value) = value.as_u128() {
        Ok(IntegerCount::Unsigned(value))
    } else {
        value
            .as_i128()
            .map(IntegerCount::Signed)
            .ok_or(ProgrammerError::Integer(IntegerError::OutOfRange))
    }
}

fn shift_count_argument(count: IntegerCount) -> Result<i128, ProgrammerError> {
    match count {
        IntegerCount::Signed(count) => Ok(count),
        IntegerCount::Unsigned(count) => {
            i128::try_from(count).map_err(|_| ProgrammerError::Integer(IntegerError::OutOfRange))
        }
    }
}

fn rotate_count_argument(count: IntegerCount, mode: IntegerMode) -> Result<i128, ProgrammerError> {
    match count {
        IntegerCount::Signed(count) if count < 0 => Ok(count),
        IntegerCount::Signed(count) => {
            Ok((count as u128 % u128::from(mode.width().bits())) as i128)
        }
        IntegerCount::Unsigned(count) => Ok((count % u128::from(mode.width().bits())) as i128),
    }
}

fn nonnegative_count(count: IntegerCount) -> Result<u128, ProgrammerError> {
    match count {
        IntegerCount::Unsigned(count) => Ok(count),
        IntegerCount::Signed(count) if count >= 0 => Ok(count as u128),
        IntegerCount::Signed(_) => Err(ProgrammerError::Integer(IntegerError::OutOfRange)),
    }
}

#[cfg(test)]
mod tests {
    use super::{ProgrammerError, parse};
    use std::collections::HashMap;

    #[test]
    fn parser_preserves_braced_unicode_references_and_operator_syntax() {
        let source = "α & {wide name}";
        let parsed = parse(source);
        let expression = parsed.expression.expect("valid programmer expression");

        assert!(parsed.uses_programmer_syntax);
        assert_eq!(parsed.identifier_count, 2);
        assert_eq!(
            expression.references(),
            vec![("α".to_owned(), false), ("wide name".to_owned(), true)]
        );
        assert_eq!(expression.root.span, 0..source.len());
    }

    #[test]
    fn parser_evaluates_large_decimal_literals_without_float_rounding() {
        let parsed = parse("9007199254740993 | 0");
        let expression = parsed.expression.expect("valid programmer expression");
        let value = expression.evaluate(&HashMap::new()).unwrap();

        assert_eq!(value.formatted_decimal(), "9007199254740993");
        assert_eq!(value.mode().type_name(), "u64");
    }

    #[test]
    fn parser_bounds_tokens_and_names() {
        let too_many_tokens = format!("1 & 1{}", "+ 1".repeat(130));
        let parsed = parse(&too_many_tokens);
        assert!(matches!(
            parsed.error,
            Some(ProgrammerError::TooManyTokens { .. })
        ));

        let too_long_name = format!("u8(1) | {{{}}}", "x".repeat(129));
        let parsed = parse(&too_long_name);
        assert!(matches!(
            parsed.error,
            Some(ProgrammerError::NameTooLong { .. })
        ));
    }
}
