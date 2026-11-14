//! Bounded, lazy expressions composed with the existing arithmetic evaluators.

use super::*;
use std::cmp::Ordering;
use std::sync::Arc;

const MAX_SEQUENCE_ELEMENTS: usize = 1024;
const MAX_WORK: usize = 50_000;

pub(super) fn validate_source(source: &str) -> Result<usize, DiagnosticKind> {
    let mut stack = Vec::new();
    let mut identifiers = 0usize;
    let mut chars = source.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        match ch {
            '{' => {
                let mut length = 0;
                loop {
                    match chars.next() {
                        Some((_, '}')) => break,
                        Some((_, '{' | '\n' | '\r')) | None => {
                            return Err(DiagnosticKind::InvalidExpression);
                        }
                        Some((_, c)) => length += c.len_utf8(),
                    }
                }
                if length == 0 {
                    return Err(DiagnosticKind::InvalidExpression);
                }
                if length > MAX_NAME_BYTES {
                    return Err(DiagnosticKind::NameTooLong {
                        max_bytes: MAX_NAME_BYTES,
                    });
                }
                identifiers += 1;
            }
            '(' | '[' => {
                stack.push(ch);
                if stack.len() > MAX_EXPRESSION_NESTING {
                    return Err(DiagnosticKind::NestingTooDeep {
                        max_depth: MAX_EXPRESSION_NESTING,
                    });
                }
            }
            ')' | ']' => {
                if stack.pop() != Some(if ch == ')' { '(' } else { '[' }) {
                    return Err(DiagnosticKind::InvalidExpression);
                }
            }
            '}' | '@' => return Err(DiagnosticKind::InvalidExpression),
            c if is_identifier_start(c) => {
                let mut length = c.len_utf8();
                while let Some((_, next)) = chars.peek().copied() {
                    if !is_identifier_continue(next) {
                        break;
                    }
                    length += next.len_utf8();
                    chars.next();
                }
                if length > MAX_NAME_BYTES {
                    return Err(DiagnosticKind::NameTooLong {
                        max_bytes: MAX_NAME_BYTES,
                    });
                }
                if !source[..start]
                    .chars()
                    .next_back()
                    .is_some_and(is_identifier_continue)
                {
                    identifiers += 1;
                }
            }
            _ => {}
        }
        if identifiers > MAX_DEPENDENCIES_PER_LINE {
            return Err(DiagnosticKind::TooManyIdentifiers {
                max_identifiers: MAX_DEPENDENCIES_PER_LINE,
            });
        }
    }
    if !stack.is_empty() {
        return Err(DiagnosticKind::InvalidExpression);
    }
    Ok(identifiers)
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalculationSequence {
    values: Arc<[f64]>,
    source: String,
}

impl CalculationSequence {
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    pub fn canonical_copy(&self) -> &str {
        &self.source
    }
    pub fn display(&self) -> String {
        if self.values.len() <= 12 {
            format!(
                "{} = [{}]",
                self.source,
                self.values
                    .iter()
                    .filter_map(|v| format_number(*v))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            format!("{} ({} values)", self.source, self.values.len())
        }
    }
}

#[derive(Default)]
pub(crate) struct WorkBudget {
    used: usize,
    cancellation: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) runtime_line: Option<usize>,
    pub(super) runtime_definitions: Option<Arc<HashMap<String, Vec<usize>>>>,
}
impl WorkBudget {
    pub(crate) fn charge(&mut self, amount: usize) -> Result<(), DiagnosticKind> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
        {
            return Err(DiagnosticKind::Canceled);
        }
        self.used = self
            .used
            .checked_add(amount)
            .ok_or(DiagnosticKind::WorkLimit)?;
        if self.used > MAX_WORK {
            return Err(DiagnosticKind::WorkLimit);
        }
        Ok(())
    }

    pub(crate) fn with_cancellation(cancellation: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self {
            used: 0,
            cancellation: Some(cancellation),
            runtime_line: None,
            runtime_definitions: None,
        }
    }
}
fn error(message: &str) -> DiagnosticKind {
    DiagnosticKind::Script(message.to_owned())
}

#[derive(Debug)]
pub(super) enum ScriptExpression {
    Boolean {
        name: String,
        target: Option<usize>,
    },
    Legacy {
        line: Box<PreparedLine>,
        slots: Vec<(String, ScriptExpression)>,
        alias: Option<String>,
    },
    If(Box<Self>, Box<Self>, Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Not(Box<Self>),
    Compare(Compare, Box<Self>, Box<Self>),
    Call(Function, Vec<Self>),
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Compare {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
#[derive(Debug, Clone, Copy)]
pub(super) enum Function {
    Range,
    Sum,
    Average,
    Median,
    Min,
    Max,
    Stdev,
}

/// The signatures are shared with editor assistance and reflect runtime arity.
pub(super) fn signature(name: &str) -> Option<&'static [usize]> {
    match name.to_ascii_lowercase().as_str() {
        "if" => Some(&[3]),
        "range" => Some(&[1, 2, 3]),
        "sum" | "average" | "median" | "stdev" => Some(&[1]),
        "min" | "max" => Some(&[1, 2]),
        "atan2" | "mod" | "dot" | "cross" | "rotl" | "rotr" => Some(&[2]),
        "mask" | "extract" => Some(&[3]),
        "abs" | "signum" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh"
        | "tanh" | "asinh" | "acosh" | "atanh" | "floor" | "round" | "ceil" | "trunc" | "fract"
        | "exp" | "sqrt" | "cbrt" | "ln" | "log2" | "log10" | "log" | "not" | "popcount"
        | "int" | "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64" | "u128" => {
            Some(&[1])
        }
        _ => None,
    }
}

fn special(name: &str, arity: usize) -> Option<Function> {
    match name.to_ascii_lowercase().as_str() {
        "range" => Some(Function::Range),
        "sum" => Some(Function::Sum),
        "average" => Some(Function::Average),
        "median" => Some(Function::Median),
        "stdev" => Some(Function::Stdev),
        "min" if arity == 1 => Some(Function::Min),
        "max" if arity == 1 => Some(Function::Max),
        _ => None,
    }
}

fn infix_word(source: &str, start: usize, name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "mod" | "and" | "or" | "xor"
    ) && source[..start]
        .trim_end()
        .chars()
        .next_back()
        .is_some_and(|ch| is_identifier_continue(ch) || matches!(ch, ')' | ']' | '}'))
}

pub(super) fn uses_script_syntax(source: &str) -> bool {
    let mut iter = source.char_indices().peekable();
    while let Some((start, ch)) = iter.next() {
        if ch == '{' {
            for (_, c) in iter.by_ref() {
                if c == '}' {
                    break;
                }
            }
            continue;
        }
        if matches!(ch, '<' | '>' | '=' | '!') {
            let before = source[..start].chars().next_back();
            let after = source[start + ch.len_utf8()..].chars().next();
            if !matches!(
                (ch, before, after),
                ('<', _, Some('<'))
                    | ('>', _, Some('>'))
                    | ('<', Some('<'), _)
                    | ('>', Some('>'), _)
            ) {
                return true;
            }
        }
        if matches!(ch, '&' | '|') && source[start + 1..].starts_with(ch) {
            return true;
        }
        if !is_identifier_start(ch) {
            continue;
        }
        if source[..start]
            .chars()
            .next_back()
            .is_some_and(is_identifier_continue)
        {
            continue;
        }
        let mut end = start + ch.len_utf8();
        while let Some((_, next)) = iter.peek().copied() {
            if !is_identifier_continue(next) {
                break;
            }
            end += next.len_utf8();
            iter.next();
        }
        let name = &source[start..end];
        if matches!(name, "true" | "false") {
            return true;
        }
        if source[end..].trim_start().starts_with('(')
            && matches!(
                name.to_ascii_lowercase().as_str(),
                "if" | "range" | "sum" | "average" | "median" | "stdev"
            )
        {
            return true;
        }
        // Unary collection min/max must be routed through script parsing.
        if matches!(name.to_ascii_lowercase().as_str(), "min" | "max")
            && source[end..].trim_start().starts_with('(')
        {
            let open = end + source[end..].len() - source[end..].trim_start().len();
            if let Ok(close) = matching(source, open) {
                if split_arguments(&source[open + 1..close]).is_ok_and(|args| args.len() == 1) {
                    return true;
                }
            }
        }
    }
    false
}

pub(super) fn prepare(
    source: &str,
    declared: &HashSet<String>,
) -> Result<ScriptExpression, DiagnosticKind> {
    parse(source.trim(), declared, 0)
}

fn matching(source: &str, open: usize) -> Result<usize, DiagnosticKind> {
    let mut stack = Vec::new();
    let mut braces = false;
    for (offset, ch) in source[open..].char_indices() {
        if braces {
            if ch == '}' {
                braces = false;
            }
            continue;
        }
        if ch == '{' {
            braces = true;
            continue;
        }
        match ch {
            '(' | '[' => stack.push(ch),
            ')' | ']' => {
                let expected = if ch == ')' { '(' } else { '[' };
                if stack.pop() != Some(expected) {
                    return Err(DiagnosticKind::InvalidExpression);
                }
                if stack.is_empty() {
                    return Ok(open + offset);
                }
            }
            _ => {}
        }
    }
    Err(DiagnosticKind::InvalidExpression)
}

fn split_arguments(source: &str) -> Result<Vec<&str>, DiagnosticKind> {
    if source.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut output = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < source.len() {
        let ch = source[index..]
            .chars()
            .next()
            .ok_or(DiagnosticKind::InvalidExpression)?;
        if matches!(ch, '(' | '[') {
            index = matching(source, index)? + 1;
            continue;
        }
        if ch == '{' {
            index += source[index..]
                .find('}')
                .ok_or(DiagnosticKind::InvalidExpression)?
                + 1;
            continue;
        }
        if matches!(ch, ')' | ']' | '}') {
            return Err(DiagnosticKind::InvalidExpression);
        }
        if ch == ',' {
            output.push(source[start..index].trim());
            start = index + 1;
        }
        index += ch.len_utf8();
    }
    output.push(source[start..].trim());
    if output.iter().any(|arg| arg.is_empty()) {
        return Err(DiagnosticKind::InvalidExpression);
    }
    Ok(output)
}

fn top_operator(
    source: &str,
    operators: &[&str],
) -> Result<Option<(usize, usize)>, DiagnosticKind> {
    let mut index = 0;
    let mut found = None;
    while index < source.len() {
        let ch = source[index..]
            .chars()
            .next()
            .ok_or(DiagnosticKind::InvalidExpression)?;
        if matches!(ch, '(' | '[') {
            index = matching(source, index)? + 1;
            continue;
        }
        if ch == '{' {
            index += source[index..]
                .find('}')
                .ok_or(DiagnosticKind::InvalidExpression)?
                + 1;
            continue;
        }
        if matches!(ch, ')' | ']' | '}') {
            return Err(DiagnosticKind::InvalidExpression);
        }
        if source[index..].starts_with("<<") || source[index..].starts_with(">>") {
            index += 2;
            continue;
        }
        if let Some((op_index, op)) = operators
            .iter()
            .enumerate()
            .find(|(_, op)| source[index..].starts_with(**op))
        {
            found = Some((index, op_index));
            index += op.len();
            continue;
        }
        index += ch.len_utf8();
    }
    Ok(found)
}

fn parse(
    source: &str,
    declared: &HashSet<String>,
    depth: usize,
) -> Result<ScriptExpression, DiagnosticKind> {
    if depth > MAX_EXPRESSION_NESTING {
        return Err(DiagnosticKind::NestingTooDeep {
            max_depth: MAX_EXPRESSION_NESTING,
        });
    }
    let source = source.trim();
    if source.is_empty() {
        return Err(DiagnosticKind::InvalidExpression);
    }
    if source.starts_with('(') && matching(source, 0)? == source.len() - 1 {
        return parse(&source[1..source.len() - 1], declared, depth + 1);
    }
    for operators in [
        &["||"][..],
        &["&&"][..],
        &["==", "!=", "<=", ">=", "<", ">"][..],
    ] {
        if let Some((at, which)) = top_operator(source, operators)? {
            let left = Box::new(parse(&source[..at], declared, depth + 1)?);
            let right = Box::new(parse(
                &source[at + operators[which].len()..],
                declared,
                depth + 1,
            )?);
            return Ok(match operators[which] {
                "||" => ScriptExpression::Or(left, right),
                "&&" => ScriptExpression::And(left, right),
                "==" => ScriptExpression::Compare(Compare::Equal, left, right),
                "!=" => ScriptExpression::Compare(Compare::NotEqual, left, right),
                "<" => ScriptExpression::Compare(Compare::Less, left, right),
                "<=" => ScriptExpression::Compare(Compare::LessEqual, left, right),
                ">" => ScriptExpression::Compare(Compare::Greater, left, right),
                ">=" => ScriptExpression::Compare(Compare::GreaterEqual, left, right),
                _ => unreachable!(),
            });
        }
    }
    if let Some(rest) = source.strip_prefix('!') {
        return Ok(ScriptExpression::Not(Box::new(parse(
            rest,
            declared,
            depth + 1,
        )?)));
    }
    if matches!(source, "true" | "false") {
        return Ok(ScriptExpression::Boolean {
            name: source.to_owned(),
            target: None,
        });
    }
    let mut template = String::with_capacity(source.len());
    let mut slots = Vec::new();
    let mut index = 0;
    while index < source.len() {
        let ch = source[index..]
            .chars()
            .next()
            .ok_or(DiagnosticKind::InvalidExpression)?;
        if ch == '{' {
            let end = index
                + source[index..]
                    .find('}')
                    .ok_or(DiagnosticKind::InvalidExpression)?
                + 1;
            template.push_str(&source[index..end]);
            index = end;
            continue;
        }
        if ch == '(' {
            let close = matching(source, index)?;
            let inner = &source[index + 1..close];
            if uses_script_syntax(inner) {
                let node = parse(inner, declared, depth + 1)?;
                add_slot(&mut template, &mut slots, node, source, declared);
            } else {
                template.push_str(&source[index..=close]);
            }
            index = close + 1;
            continue;
        }
        if is_identifier_start(ch)
            && (index == 0
                || !source[..index]
                    .chars()
                    .next_back()
                    .is_some_and(is_identifier_continue))
        {
            let mut end = index + ch.len_utf8();
            while end < source.len() {
                let next = source[end..].chars().next().unwrap();
                if !is_identifier_continue(next) {
                    break;
                }
                end += next.len_utf8();
            }
            let name = &source[index..end];
            if matches!(name, "true" | "false") {
                add_slot(
                    &mut template,
                    &mut slots,
                    ScriptExpression::Boolean {
                        name: name.to_owned(),
                        target: None,
                    },
                    source,
                    declared,
                );
                index = end;
                continue;
            }
            let open = end + source[end..].len() - source[end..].trim_start().len();
            if infix_word(source, index, name) {
                template.push_str(name);
                index = end;
                continue;
            }
            if source[open..].starts_with('(') {
                let close = matching(source, open)?;
                let args = split_arguments(&source[open + 1..close])?;
                let allowed = signature(name).ok_or(DiagnosticKind::InvalidExpression)?;
                if !allowed.contains(&args.len()) {
                    return Err(DiagnosticKind::InvalidExpression);
                }
                let is_if = name.eq_ignore_ascii_case("if");
                if is_if || special(name, args.len()).is_some() {
                    let children = args
                        .into_iter()
                        .map(|arg| parse(arg, declared, depth + 1))
                        .collect::<Result<Vec<_>, _>>()?;
                    let node = if is_if {
                        let [first, second, third]: [ScriptExpression; 3] = children
                            .try_into()
                            .map_err(|_| DiagnosticKind::InvalidExpression)?;
                        ScriptExpression::If(Box::new(first), Box::new(second), Box::new(third))
                    } else {
                        let function = special(name, children.len())
                            .ok_or(DiagnosticKind::InvalidExpression)?;
                        ScriptExpression::Call(function, children)
                    };
                    if index == 0 && close + 1 == source.len() {
                        return Ok(node);
                    }
                    add_slot(&mut template, &mut slots, node, source, declared);
                    index = close + 1;
                    continue;
                }
                template.push_str(&source[index..=open]);
                let mut rendered = Vec::new();
                for arg in args {
                    if uses_script_syntax(arg) {
                        let mut slot = String::new();
                        add_slot(
                            &mut slot,
                            &mut slots,
                            parse(arg, declared, depth + 1)?,
                            source,
                            declared,
                        );
                        rendered.push(slot);
                    } else {
                        validate_calls(arg)?;
                        rendered.push(arg.to_owned());
                    }
                }
                template.push_str(&rendered.join(", "));
                template.push(')');
                index = close + 1;
                continue;
            }
            template.push_str(name);
            index = end;
            continue;
        }
        template.push(ch);
        index += ch.len_utf8();
    }
    validate_calls(&template)?;
    let mut all_declared = declared.clone();
    all_declared.extend(slots.iter().map(|(name, _)| name.clone()));
    let mut count = 0;
    let line = prepare_line_internal(
        0..template.len(),
        &template,
        &mut count,
        &all_declared,
        true,
    );
    if let Some(diagnostic) = &line.diagnostic {
        if !matches!(
            diagnostic.kind,
            DiagnosticKind::InvalidScientificLiteral
                | DiagnosticKind::ScientificLiteralOutOfRange
                | DiagnosticKind::PrefixedIntegerOverflow { .. }
                | DiagnosticKind::PrefixedIntegerTooLarge { .. }
                | DiagnosticKind::ScalarIntegerTooLarge { .. }
        ) {
            return Err(diagnostic.kind.clone());
        }
        if programmer::parse(&template).expression.is_none() {
            return Err(DiagnosticKind::InvalidExpression);
        }
    }
    let alias = parse_simple_reference(source).map(|reference| reference.name);
    Ok(ScriptExpression::Legacy {
        line: Box::new(line),
        slots,
        alias,
    })
}

fn add_slot(
    template: &mut String,
    slots: &mut Vec<(String, ScriptExpression)>,
    node: ScriptExpression,
    source: &str,
    declared: &HashSet<String>,
) {
    let mut number = slots.len();
    let name = loop {
        let name = format!("__pad_script_slot_{number}");
        if !source.contains(&name)
            && !declared.contains(&name)
            && !slots.iter().any(|(existing, _)| existing == &name)
        {
            break name;
        }
        number += 1;
    };
    template.push_str(&name);
    slots.push((name, node));
}

fn validate_calls(source: &str) -> Result<(), DiagnosticKind> {
    let mut index = 0;
    while index < source.len() {
        let ch = source[index..]
            .chars()
            .next()
            .ok_or(DiagnosticKind::InvalidExpression)?;
        if ch == '{' {
            index += source[index..]
                .find('}')
                .ok_or(DiagnosticKind::InvalidExpression)?
                + 1;
            continue;
        }
        if is_identifier_start(ch)
            && (index == 0
                || !source[..index]
                    .chars()
                    .next_back()
                    .is_some_and(is_identifier_continue))
        {
            let mut end = index + ch.len_utf8();
            while end < source.len() {
                let next = source[end..].chars().next().unwrap();
                if !is_identifier_continue(next) {
                    break;
                }
                end += next.len_utf8();
            }
            let open = end + source[end..].len() - source[end..].trim_start().len();
            if infix_word(source, index, &source[index..end]) {
                index = end;
                continue;
            }
            if source[open..].starts_with('(') {
                let close = matching(source, open)?;
                let args = split_arguments(&source[open + 1..close])?;
                if !signature(&source[index..end])
                    .is_some_and(|arities| arities.contains(&args.len()))
                {
                    return Err(DiagnosticKind::InvalidExpression);
                }
                for arg in args {
                    validate_calls(arg)?;
                }
                index = close + 1;
                continue;
            }
            index = end;
            continue;
        }
        index += ch.len_utf8();
    }
    Ok(())
}

impl ScriptExpression {
    pub(super) fn resolve(
        &mut self,
        line_index: usize,
        row_count: usize,
        mut named_binding: impl FnMut(&str) -> Option<usize>,
    ) {
        self.resolve_inner(line_index, row_count, &mut named_binding);
    }

    fn resolve_inner(
        &mut self,
        line_index: usize,
        row_count: usize,
        named_binding: &mut impl FnMut(&str) -> Option<usize>,
    ) {
        match self {
            Self::Boolean { name, target } => {
                *target = named_binding(name)
                    .filter(|target| *target < line_index || *target >= super::MAX_SOURCE_LINES);
            }
            Self::Legacy { line, slots, .. } => {
                for (_, child) in slots.iter_mut() {
                    child.resolve_inner(line_index, row_count, named_binding);
                }
                for reference in &mut line.references {
                    if slots.iter().any(|(name, _)| name == &reference.name) {
                        continue;
                    }
                    reference.target_line = match reference.kind {
                        ReferenceKind::Named => named_binding(&reference.name),
                        ReferenceKind::Row => {
                            reference.target_line.filter(|target| *target < row_count)
                        }
                        ReferenceKind::InvalidRow => None,
                    };
                }
            }
            Self::If(a, b, c) => {
                a.resolve_inner(line_index, row_count, named_binding);
                b.resolve_inner(line_index, row_count, named_binding);
                c.resolve_inner(line_index, row_count, named_binding);
            }
            Self::And(a, b) | Self::Or(a, b) | Self::Compare(_, a, b) => {
                a.resolve_inner(line_index, row_count, named_binding);
                b.resolve_inner(line_index, row_count, named_binding);
            }
            Self::Not(child) => child.resolve_inner(line_index, row_count, named_binding),
            Self::Call(_, args) => {
                for arg in args {
                    arg.resolve_inner(line_index, row_count, named_binding);
                }
            }
        }
    }

    pub(super) fn evaluate(
        &self,
        line_index: usize,
        outcomes: &[LineOutcome],
        values: &[Option<CalculationValue>],
        dependencies: &mut Vec<usize>,
        work: &mut WorkBudget,
    ) -> Result<CalculationValue, DiagnosticKind> {
        work.charge(1)?;
        match self {
            Self::Boolean { name, target } => {
                if let Some(target) = target {
                    dependencies.push(*target);
                    return value_at(name, *target, line_index, outcomes, values, work, true);
                }
                Ok(CalculationValue::Boolean(name == "true"))
            }
            Self::Legacy { line, slots, alias } => {
                work.charge(line.source_range.len().max(1))?;
                if let Some(diagnostic) = &line.diagnostic {
                    return Err(diagnostic.kind.clone());
                }
                let mut slot_values = Vec::with_capacity(slots.len());
                for (name, child) in slots {
                    slot_values.push((
                        name,
                        child.evaluate(line_index, outcomes, values, dependencies, work)?,
                    ));
                }
                let mut references = line.references.clone();
                let mut all_values = Vec::with_capacity(references.len());
                for reference in &mut references {
                    if let Some((_, value)) = slot_values
                        .iter()
                        .find(|(name, _)| **name == reference.name)
                    {
                        reference.target_line = Some(all_values.len());
                        all_values.push(Some(value.clone()));
                        continue;
                    }
                    let Some(target) = reference.target_line else {
                        return Err(match reference.kind {
                            ReferenceKind::Named => {
                                DiagnosticKind::UnknownName(reference.name.clone())
                            }
                            _ => DiagnosticKind::UnknownRowReference(reference.name.clone()),
                        });
                    };
                    dependencies.push(target);
                    let value = value_at(&reference.name, target, line_index, outcomes, values, work, false)?;
                    reference.target_line = Some(all_values.len());
                    all_values.push(Some(value));
                }
                if let Some(name) = alias
                    && let Some(reference) =
                        references.iter().find(|reference| &reference.name == name)
                    && let Some(value) = reference
                        .target_line
                        .and_then(|target| all_values.get(target))
                        .and_then(Option::as_ref)
                    && matches!(
                        value,
                        CalculationValue::Boolean(_) | CalculationValue::Sequence(_)
                    )
                {
                    return Ok(value.clone());
                }
                let surrogate = PreparedLine {
                    source_range: line.source_range.clone(),
                    assignment_name: None,
                    statement: PreparedStatement::Invalid,
                    references,
                    dependencies: Vec::new(),
                    diagnostic: None,
                };
                evaluate_prepared_statement(&line.statement, &surrogate, &all_values)
            }
            Self::If(condition, yes, no) => {
                let condition = boolean(condition.evaluate(
                    line_index,
                    outcomes,
                    values,
                    dependencies,
                    work,
                )?)?;
                if condition {
                    yes.evaluate(line_index, outcomes, values, dependencies, work)
                } else {
                    no.evaluate(line_index, outcomes, values, dependencies, work)
                }
            }
            Self::And(left, right) => {
                let left =
                    boolean(left.evaluate(line_index, outcomes, values, dependencies, work)?)?;
                if !left {
                    return Ok(CalculationValue::Boolean(false));
                }
                Ok(CalculationValue::Boolean(boolean(right.evaluate(
                    line_index,
                    outcomes,
                    values,
                    dependencies,
                    work,
                )?)?))
            }
            Self::Or(left, right) => {
                let left =
                    boolean(left.evaluate(line_index, outcomes, values, dependencies, work)?)?;
                if left {
                    return Ok(CalculationValue::Boolean(true));
                }
                Ok(CalculationValue::Boolean(boolean(right.evaluate(
                    line_index,
                    outcomes,
                    values,
                    dependencies,
                    work,
                )?)?))
            }
            Self::Not(child) => Ok(CalculationValue::Boolean(!boolean(child.evaluate(
                line_index,
                outcomes,
                values,
                dependencies,
                work,
            )?)?)),
            Self::Compare(operator, left, right) => {
                let left = left.evaluate(line_index, outcomes, values, dependencies, work)?;
                let right = right.evaluate(line_index, outcomes, values, dependencies, work)?;
                let ordering = compare_values(&left, &right)?;
                let result = match operator {
                    Compare::Equal => ordering == Ordering::Equal,
                    Compare::NotEqual => ordering != Ordering::Equal,
                    Compare::Less => ordering == Ordering::Less,
                    Compare::LessEqual => ordering != Ordering::Greater,
                    Compare::Greater => ordering == Ordering::Greater,
                    Compare::GreaterEqual => ordering != Ordering::Less,
                };
                Ok(CalculationValue::Boolean(result))
            }
            Self::Call(function, args) => {
                let values = args
                    .iter()
                    .map(|arg| arg.evaluate(line_index, outcomes, values, dependencies, work))
                    .collect::<Result<Vec<_>, _>>()?;
                call(*function, &values, work)
            }
        }
    }
}

fn value_at(
    name: &str,
    target: usize,
    line_index: usize,
    outcomes: &[LineOutcome],
    values: &[Option<CalculationValue>],
    work: &WorkBudget,
    boolean_literal: bool,
) -> Result<CalculationValue, DiagnosticKind> {
    let physical_line = work.runtime_line.unwrap_or(line_index);
    let runtime_binding = work.runtime_line.is_some() && target >= super::MAX_SOURCE_LINES;
    if runtime_binding && matches!(
        outcomes.get(target),
        Some(LineOutcome::Error(Diagnostic { kind: DiagnosticKind::NotExecuted }))
    ) {
        if boolean_literal && (name == "true" || name == "false") {
            return Ok(CalculationValue::Boolean(name == "true"));
        }
        if let Some(location) = work.runtime_definitions.as_ref()
            .and_then(|definitions| definitions.get(name))
            .and_then(|locations| locations.iter().find(|&&location| location >= physical_line))
        {
            if *location == physical_line {
                return Err(DiagnosticKind::SelfReference(name.to_owned()));
            }
            return Err(DiagnosticKind::ForwardReference {
                reference: name.to_owned(),
                dependency_line: *location,
            });
        }
        return Err(DiagnosticKind::UnknownName(name.to_owned()));
    }
    if !runtime_binding && target == physical_line {
        return Err(DiagnosticKind::SelfReference(name.to_owned()));
    }
    if !runtime_binding && target > physical_line {
        return Err(DiagnosticKind::ForwardReference {
            reference: name.to_owned(),
            dependency_line: target,
        });
    }
    match outcomes.get(target) {
        Some(LineOutcome::Value(_)) => values.get(target).and_then(Option::as_ref).cloned().ok_or(
            DiagnosticKind::DependencyFailed {
                dependency_line: target,
            },
        ),
        Some(LineOutcome::Blank | LineOutcome::Comment) => {
            Err(DiagnosticKind::UnknownRowReference(name.to_owned()))
        }
        _ => Err(DiagnosticKind::DependencyFailed {
            dependency_line: target,
        }),
    }
}
fn boolean(value: CalculationValue) -> Result<bool, DiagnosticKind> {
    match value {
        CalculationValue::Boolean(value) => Ok(value),
        _ => Err(error("condition requires a Boolean value")),
    }
}

fn compare_values(
    left: &CalculationValue,
    right: &CalculationValue,
) -> Result<Ordering, DiagnosticKind> {
    match (left, right) {
        (CalculationValue::Boolean(a), CalculationValue::Boolean(b)) => Ok(a.cmp(b)),
        (CalculationValue::Scalar(a), CalculationValue::Scalar(b)) => a
            .partial_cmp(b)
            .ok_or_else(|| error("values cannot be compared")),
        (CalculationValue::Integer(a), CalculationValue::Integer(b)) if a.mode() == b.mode() => {
            if a.mode().is_signed() {
                Ok(a.as_i128().cmp(&b.as_i128()))
            } else {
                Ok(a.bit_pattern().cmp(&b.bit_pattern()))
            }
        }
        (
            CalculationValue::ExactBase { value: a, .. },
            CalculationValue::ExactBase { value: b, .. },
        ) => Ok(a.cmp(b)),
        (
            CalculationValue::PhysicalQuantity { .. } | CalculationValue::DimensionalQuantity(_),
            CalculationValue::PhysicalQuantity { .. } | CalculationValue::DimensionalQuantity(_),
        ) => quantity::compare_ordering(left, right).map_err(DiagnosticKind::Quantity),
        _ => Err(error("values have incompatible comparison types")),
    }
}

fn call(
    function: Function,
    arguments: &[CalculationValue],
    work: &mut WorkBudget,
) -> Result<CalculationValue, DiagnosticKind> {
    if matches!(function, Function::Range) {
        let numbers = arguments
            .iter()
            .map(|argument| match argument {
                CalculationValue::Scalar(value) if value.is_finite() && value.fract() == 0.0 => {
                    Ok(*value)
                }
                _ => Err(error("range arguments must be finite integral scalars")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (start, stop, step) = match numbers.as_slice() {
            [stop] => (0.0, *stop, 1.0),
            [start, stop] => (*start, *stop, 1.0),
            [start, stop, step] => (*start, *stop, *step),
            _ => return Err(DiagnosticKind::InvalidExpression),
        };
        if step == 0.0 {
            return Err(error("range step cannot be zero"));
        }
        let mut current = start;
        let mut count = 0usize;
        while if step > 0.0 {
            current < stop
        } else {
            current > stop
        } {
            work.charge(1)?;
            count += 1;
            if count > MAX_SEQUENCE_ELEMENTS {
                return Err(error("range exceeds the 1024-element limit"));
            }
            let next = current + step;
            if !next.is_finite() {
                break;
            }
            let virtual_step = next - current;
            let residual = (current - (next - virtual_step)) + (step - virtual_step);
            if residual != 0.0 {
                let still_inside = if step > 0.0 {
                    next < stop || (next == stop && residual < 0.0)
                } else {
                    next > stop || (next == stop && residual > 0.0)
                };
                if still_inside {
                    return Err(error(
                        "range contains an integer that cannot be represented exactly",
                    ));
                }
                break;
            }
            current = next;
        }
        work.charge(count)?;
        let mut values = Vec::with_capacity(count);
        current = start;
        for _ in 0..count {
            values.push(current);
            current += step;
        }
        let source = format!("range({start:?}, {stop:?}, {step:?})");
        return Ok(CalculationValue::Sequence(CalculationSequence {
            values: values.into(),
            source,
        }));
    }
    let values: &[f64] = match arguments {
        [CalculationValue::Vector(value)] => value.components(),
        [CalculationValue::Sequence(value)] => value.values(),
        _ => {
            return Err(error(
                "collection function requires a scalar vector or sequence",
            ));
        }
    };
    work.charge(values.len())?;
    if values.is_empty() {
        return if matches!(function, Function::Sum) {
            Ok(CalculationValue::Scalar(0.0))
        } else {
            Err(error("collection must not be empty"))
        };
    }
    let result = match function {
        Function::Sum => values
            .iter()
            .try_fold(0.0, |sum, value| finite(sum + value))?,
        Function::Average => {
            let mut mean = 0.0;
            for (index, value) in values.iter().enumerate() {
                mean = finite(mean + (value - mean) / (index + 1) as f64)?;
            }
            mean
        }
        Function::Median => {
            let sort_work = values
                .len()
                .saturating_mul((usize::BITS - values.len().leading_zeros()) as usize);
            work.charge(sort_work)?;
            let mut sorted = values.to_vec();
            sorted.sort_by(f64::total_cmp);
            let middle = sorted.len() / 2;
            if sorted.len() % 2 == 1 {
                sorted[middle]
            } else {
                finite(sorted[middle - 1].midpoint(sorted[middle]))?
            }
        }
        Function::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
        Function::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        Function::Stdev => {
            let mut mean = 0.0;
            let mut variance_sum = 0.0;
            for (index, value) in values.iter().enumerate() {
                let delta = finite(*value - mean)?;
                mean = finite(mean + delta / (index + 1) as f64)?;
                variance_sum = finite(variance_sum + delta * (*value - mean))?;
            }
            finite((variance_sum / values.len() as f64).max(0.0).sqrt())?
        }
        Function::Range => unreachable!(),
    };
    Ok(CalculationValue::Scalar(finite(result)?))
}
fn finite(value: f64) -> Result<f64, DiagnosticKind> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(DiagnosticKind::InvalidArithmetic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evaluated(source: &str) -> Vec<LineOutcome> {
        CalculationDocument::new(source)
            .evaluate()
            .lines
            .into_iter()
            .map(|line| line.outcome)
            .collect()
    }
    fn scalar(value: &LineOutcome) -> f64 {
        match value {
            LineOutcome::Value(CalculationValue::Scalar(value)) => *value,
            other => panic!("expected scalar: {other:?}"),
        }
    }
    fn boolean_value(value: &LineOutcome) -> bool {
        match value {
            LineOutcome::Value(CalculationValue::Boolean(value)) => *value,
            other => panic!("expected Boolean: {other:?}"),
        }
    }

    #[test]
    fn lazy_conditions_are_composable_and_track_only_executed_dependencies() {
        let document = CalculationDocument::new(concat!(
            "x = 3\n",
            "if(x > 0, 7, 1 / 0)\n",
            "false && (missing > 0)\n",
            "true || (1 / 0 > 0)\n",
            "round(if(x == 3, 2.6, 9))\n",
            "[if(x > 0, 2, 9), 4]\n",
            "u64(1) + if(x == 3, u64(2), u64(3))\n",
            "if(true, 7, round(1, 2))\n",
            "!(x > 0) == false\n",
        ));
        let result = document.evaluate();
        assert_eq!(scalar(&result.lines[1].outcome), 7.0);
        assert_eq!(result.lines[1].dependency_indices, vec![0]);
        assert!(!boolean_value(&result.lines[2].outcome));
        assert!(result.lines[2].dependency_indices.is_empty());
        assert!(boolean_value(&result.lines[3].outcome));
        assert_eq!(scalar(&result.lines[4].outcome), 3.0);
        assert!(
            matches!(&result.lines[5].outcome, LineOutcome::Value(CalculationValue::Vector(value)) if value.components() == [2.0, 4.0])
        );
        assert!(
            matches!(&result.lines[6].outcome, LineOutcome::Value(CalculationValue::Integer(value)) if value.bit_pattern() == 3)
        );
        assert!(matches!(result.lines[7].outcome, LineOutcome::Error(_)));
        assert!(boolean_value(&result.lines[8].outcome));
    }

    #[test]
    fn sequences_statistics_and_copy_are_bounded() {
        let values = evaluated(concat!(
            "values = range(1, 6)\n",
            "sum(values) / 3\n",
            "average([10,20,30])\n",
            "median([10,20,30])\n",
            "stdev([2,2])\n",
            "min([3,1])\n",
            "max([3,1])\n",
            "sum(range(0))\n",
            "average(range(0))\n",
            "range(3,0,-1)\n",
            "range(1024)\n",
            "range(1025)\n",
            "range(1,3,0)\n"
        ));
        assert_eq!(scalar(&values[1]), 5.0);
        assert_eq!(scalar(&values[2]), 20.0);
        assert_eq!(scalar(&values[3]), 20.0);
        assert_eq!(scalar(&values[4]), 0.0);
        assert_eq!(scalar(&values[5]), 1.0);
        assert_eq!(scalar(&values[6]), 3.0);
        assert_eq!(scalar(&values[7]), 0.0);
        assert!(matches!(values[8], LineOutcome::Error(_)));
        assert!(
            matches!(&values[9], LineOutcome::Value(CalculationValue::Sequence(value)) if value.values() == [3.0, 2.0, 1.0])
        );
        let LineOutcome::Value(sequence @ CalculationValue::Sequence(_)) = &values[10] else {
            panic!("expected sequence")
        };
        assert!(sequence.formatted_result().unwrap().contains("1024 values"));
        assert_eq!(
            scalar(&evaluated(&format!("sum({})", sequence.canonical_result().unwrap()))[0]),
            523776.0
        );
        assert!(matches!(values[11], LineOutcome::Error(_)));
        assert!(matches!(values[12], LineOutcome::Error(_)));
    }

    #[test]
    fn exact_types_contextual_names_and_nested_slots_remain_distinct() {
        let values = evaluated(concat!(
            "__pad_script_slot_0 = 0\n",
            "sum(range(2)) + sum(range(3))\n",
            "if(true, u64(0x20000000000001), u64(0))\n",
            "u64(9007199254740993) > u64(9007199254740992)\n",
            "if(true, 7, 1e999)\n",
            "if(true, 7 mod (3), 0)\n",
            "if(true, u8(3) and (u8(1)), u8(0))\n",
            "{PI} = 7\n",
            "PI\n",
            "{PI}\n",
            "true = 8\n",
            "if(1 < 2, true, 0)\n",
            "sum(range(3)) * 1 m to cm\n",
            "1 m < 200 cm\n",
            "1 m < 1 s\n",
            "if(1 < 2, 1 m, 2 m) to cm\n",
            "if(1 < 2, ff hex to dec, 0 hex to dec) to hex\n",
            "if(1 < 2, 7, 1e+)\n",
        ));
        assert_eq!(scalar(&values[1]), 4.0);
        assert!(
            matches!(&values[2], LineOutcome::Value(CalculationValue::Integer(value)) if value.bit_pattern() == 9007199254740993)
        );
        assert!(boolean_value(&values[3]));
        assert_eq!(scalar(&values[4]), 7.0);
        assert_eq!(scalar(&values[5]), 1.0);
        assert!(
            matches!(&values[6], LineOutcome::Value(CalculationValue::Integer(value)) if value.bit_pattern() == 1)
        );
        assert_eq!(scalar(&values[8]), std::f64::consts::PI);
        assert_eq!(scalar(&values[9]), 7.0);
        assert_eq!(scalar(&values[11]), 8.0);
        assert!(matches!(
            &values[12],
            LineOutcome::Value(CalculationValue::DimensionalQuantity(_))
        ));
        assert!(boolean_value(&values[13]));
        assert!(matches!(values[14], LineOutcome::Error(_)));
        assert!(
            matches!(&values[15], LineOutcome::Value(value @ (CalculationValue::DimensionalQuantity(_) | CalculationValue::PhysicalQuantity { .. })) if value.formatted_result().as_deref() == Some("100 cm")),
            "{:?}",
            values[15]
        );
        assert!(
            matches!(&values[16], LineOutcome::Value(CalculationValue::ExactBase { digits, .. }) if digits == "ff"),
            "{:?}",
            values[16]
        );
        assert!(matches!(values[17], LineOutcome::Error(_)));
    }

    #[test]
    fn work_budget_is_checked_and_range_inputs_are_not_saturated() {
        let mut work = WorkBudget::default();
        assert!(work.charge(MAX_WORK).is_ok());
        assert!(matches!(work.charge(1), Err(DiagnosticKind::WorkLimit)));
        let canceled = Arc::new(std::sync::atomic::AtomicBool::new(true));
        assert!(matches!(
            WorkBudget::with_cancellation(canceled).charge(1),
            Err(DiagnosticKind::Canceled)
        ));
        let values = evaluated("range(1e40, 1e40)\nrange(1e40, 1e41, 1)\n");
        assert!(
            matches!(&values[0], LineOutcome::Value(CalculationValue::Sequence(value)) if value.values().is_empty())
        );
        assert!(matches!(values[1], LineOutcome::Error(_)));
    }
}
