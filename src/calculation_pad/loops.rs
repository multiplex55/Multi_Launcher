//! Indentation-delimited, bounded loops over one document revision.

use super::*;
use std::sync::{Arc, atomic::AtomicBool};

pub(crate) const MAX_LOOP_DEPTH: usize = 8;
pub(crate) const MAX_LOOP_ITERATIONS: usize = 10_000;
pub(crate) const MAX_BLOCK_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BranchKind { If, Elif, Else }

struct BranchHeader {
    kind: BranchKind,
    expression: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow { Next, Break, Continue }

struct Branch {
    line: usize,
    body: Vec<Statement>,
    otherwise: bool,
}

fn branch_header(source: &str) -> Option<Result<BranchHeader, DiagnosticKind>> {
    let text = source.trim();
    if text == "else:" {
        return Some(Ok(BranchHeader { kind: BranchKind::Else, expression: None }));
    }
    for (keyword, kind) in [("if", BranchKind::If), ("elif", BranchKind::Elif)] {
        let Some(rest) = text.strip_prefix(keyword) else { continue; };
        if !(rest.starts_with(char::is_whitespace) || rest.starts_with('(')) || !rest.trim_end().ends_with(':') {
            continue;
        }
        let expression = rest.trim_end().strip_suffix(':').unwrap().trim();
        if expression.is_empty() || find_assignment_delimiter(expression).is_some() {
            return Some(Err(DiagnosticKind::Script("conditional header requires a Boolean expression".into())));
        }
        return Some(Ok(BranchHeader { kind, expression: Some(expression.to_owned()) }));
    }
    None
}

fn flow_statement(source: &str) -> Option<Flow> {
    match source.trim() {
        "break" => Some(Flow::Break),
        "continue" => Some(Flow::Continue),
        _ => None,
    }
}

pub(super) fn editor_block_header(source: &str) -> bool {
    header(source).is_some() || branch_header(source).is_some()
}

pub(super) fn editor_control_keyword(source: &str) -> Option<&'static str> {
    if let Some(Ok(header)) = branch_header(source) {
        return Some(match header.kind { BranchKind::If => "if", BranchKind::Elif => "elif", BranchKind::Else => "else" });
    }
    match flow_statement(source) {
        Some(Flow::Break) => Some("break"),
        Some(Flow::Continue) => Some("continue"),
        _ => None,
    }
}

struct LoopHeader {
    name: String,
    braced: bool,
    expression: String,
}

pub(super) fn editor_iterator(source: &str) -> Option<String> {
    header(source).and_then(Result::ok).map(|header| {
        if header.braced { format!("{{{}}}", header.name) } else { header.name }
    })
}

enum Statement {
    Line(usize),
    Definition,
    Flow { line: usize, flow: Flow },
    Conditional { branches: Vec<Branch> },
    Loop {
        line: usize,
        name: String,
        body: Vec<Statement>,
    },
}

#[derive(Clone)]
struct Binding {
    line: usize,
    outcome: LineOutcome,
}

struct Fault {
    line: usize,
    kind: DiagnosticKind,
    iteration: Vec<usize>,
}

fn affected_rows(statements: &[Statement], output: &mut Vec<usize>) {
    for statement in statements {
        match statement {
            Statement::Line(line) => output.push(*line),
            Statement::Definition => {}
            Statement::Flow { line, .. } => output.push(*line),
            Statement::Conditional { branches } => {
                for branch in branches {
                    output.push(branch.line);
                    affected_rows(&branch.body, output);
                }
            }
            Statement::Loop { line, body, .. } => {
                output.push(*line);
                affected_rows(body, output);
            }
        }
    }
}

fn header(source: &str) -> Option<Result<LoopHeader, DiagnosticKind>> {
    let rest = source.trim().strip_prefix("for")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    if rest.starts_with('=')
        || rest.starts_with(|c: char| !c.is_alphabetic() && c != '_' && c != '{')
    {
        return None;
    }
    let mut braces = false;
    let mut separator = None;
    for (index, character) in rest.char_indices() {
        match character {
            '{' => braces = true,
            '}' => braces = false,
            ' ' if !braces && rest[index..].starts_with(" in ") => {
                separator = Some(index);
                break;
            }
            _ => {}
        }
    }
    let Some(separator) = separator else {
        if !rest.trim_end().ends_with(':') {
            return None;
        }
        return Some(Err(DiagnosticKind::Script("malformed for header".into())));
    };
    let name = &rest[..separator];
    let expression = &rest[separator + 4..];
    let Some(expression) = expression.trim().strip_suffix(':') else {
        return Some(Err(DiagnosticKind::Script(
            "for header requires a trailing colon".into(),
        )));
    };
    if find_assignment_delimiter(expression)
        .is_some_and(|delimiter| delimiter.operator != AssignmentOperator::Assign)
    {
        return Some(Err(DiagnosticKind::InvalidExpression));
    }
    let assignment = match parse_assignment_name(name.trim()) {
        Ok(assignment) => assignment,
        Err(error) => return Some(Err(error)),
    };
    if is_row_reference_name(&assignment.canonical)
        || (!assignment.braced && is_reserved_name(&assignment.canonical))
    {
        return Some(Err(DiagnosticKind::ReservedName(assignment.canonical)));
    }
    Some(Ok(LoopHeader {
        name: assignment.canonical,
        braced: assignment.braced,
        expression: expression.trim().to_owned(),
    }))
}

pub(super) fn contains_loop_in(analysis: &syntax::SourceAnalysis) -> bool {
    analysis.rows.iter().any(|row| row.owner.is_none() && (
        header(&row.text).is_some() || branch_header(&row.text).is_some() || flow_statement(&row.text).is_some()
    ))
}

fn indentation(line: &str) -> Result<usize, DiagnosticKind> {
    let prefix = line.len() - line.trim_start().len();
    if line[..prefix].chars().any(|character| character != ' ') {
        return Err(DiagnosticKind::Script(
            "loop indentation must use spaces".into(),
        ));
    }
    Ok(prefix)
}

fn next_statement(lines: &[&str], mut index: usize) -> usize {
    while index < lines.len() {
        let text = lines[index].trim();
        if !text.is_empty() && !is_full_line_comment(text) {
            break;
        }
        index += 1;
    }
    index
}

fn block(
    lines: &[&str],
    headers: &[Option<Result<LoopHeader, DiagnosticKind>>],
    function_rows: &[Option<Result<(), DiagnosticKind>>],
    errors: &mut [Option<DiagnosticKind>],
    index: &mut usize,
    indent: usize,
    loop_depth: usize,
    block_depth: usize,
) -> Vec<Statement> {
    let mut statements = Vec::new();
    while *index < lines.len() {
        *index = next_statement(lines, *index);
        if *index == lines.len() {
            break;
        }
        let line = *index;
        let current = match indentation(lines[line]) {
            Ok(value) => value,
            Err(error) => {
                errors[line] = Some(error);
                *index += 1;
                statements.push(Statement::Line(line));
                continue;
            }
        };
        if current < indent {
            break;
        }
        if current > indent {
            errors[line] = Some(DiagnosticKind::Script("unexpected indentation".into()));
            *index += 1;
            statements.push(Statement::Line(line));
            continue;
        }
        *index += 1;
        if let Some(definition) = &function_rows[line] {
            if definition.is_ok() {
                statements.push(Statement::Definition);
            } else {
                statements.push(Statement::Line(line));
            }
            continue;
        }
        if let Some(flow) = flow_statement(lines[line]) {
            if loop_depth == 0 {
                errors[line] = Some(DiagnosticKind::Script("break and continue require a loop".into()));
                statements.push(Statement::Line(line));
            } else {
                statements.push(Statement::Flow { line, flow });
            }
            continue;
        }
        if let Some(branch) = branch_header(lines[line]) {
            match branch {
                Ok(branch) if branch.kind == BranchKind::If => {
                    let mut branches = Vec::new();
                    let mut branch_line = line;
                    let mut current_branch = branch;
                    loop {
                        let next = next_statement(lines, *index);
                        let body_indent = lines.get(next).and_then(|line| indentation(line).ok());
                        let body = if block_depth >= MAX_BLOCK_DEPTH {
                            errors[branch_line] = Some(DiagnosticKind::Script("block nesting exceeds 32 levels".into()));
                            Vec::new()
                        } else if let Some(body_indent) = body_indent.filter(|value| *value > indent) {
                            block(lines, headers, function_rows, errors, index, body_indent, loop_depth, block_depth + 1)
                        } else {
                            errors[branch_line] = Some(DiagnosticKind::Script("conditional branch requires an indented body".into()));
                            Vec::new()
                        };
                        let otherwise = current_branch.kind == BranchKind::Else;
                        branches.push(Branch { line: branch_line, body, otherwise });
                        if otherwise { break; }
                        let next = next_statement(lines, *index);
                        let Some(next_line) = lines.get(next) else { break; };
                        if indentation(next_line).ok() != Some(indent) { break; }
                        let Some(Ok(next_branch)) = branch_header(next_line) else { break; };
                        if next_branch.kind == BranchKind::If { break; }
                        *index = next + 1;
                        branch_line = next;
                        current_branch = next_branch;
                    }
                    statements.push(Statement::Conditional { branches });
                }
                Ok(_) => {
                    errors[line] = Some(DiagnosticKind::Script("orphan elif or else branch".into()));
                    statements.push(Statement::Line(line));
                }
                Err(error) => {
                    errors[line] = Some(error);
                    statements.push(Statement::Line(line));
                }
            }
            continue;
        }
        match &headers[line] {
            Some(Ok(header)) => {
                let next = next_statement(lines, *index);
                let body_indent = lines.get(next).and_then(|line| indentation(line).ok());
                if loop_depth >= MAX_LOOP_DEPTH || block_depth >= MAX_BLOCK_DEPTH {
                    errors[line] = Some(DiagnosticKind::Script(
                        "loop nesting exceeds 8 levels".into(),
                    ));
                    statements.push(Statement::Line(line));
                } else if let Some(body_indent) = body_indent.filter(|value| *value > indent) {
                    let body = block(lines, headers, function_rows, errors, index, body_indent, loop_depth + 1, block_depth + 1);
                    statements.push(Statement::Loop {
                        line,
                        name: header.name.clone(),
                        body,
                    });
                } else {
                    errors[line] = Some(DiagnosticKind::Script(
                        "for block requires an indented body".into(),
                    ));
                    statements.push(Statement::Line(line));
                }
            }
            Some(Err(error)) => {
                errors[line] = Some(error.clone());
                statements.push(Statement::Line(line));
            }
            None => statements.push(Statement::Line(line)),
        }
    }
    statements
}

struct Execution {
    expressions: Vec<Option<script::ScriptExpression>>,
    compound_targets: Vec<Option<String>>,
    cyclic_candidates: Vec<bool>,
    parse_errors: Vec<Option<DiagnosticKind>>,
    rows: Vec<CalculationLineResult>,
    execution_counts: Vec<usize>,
    bindings: HashMap<String, Binding>,
    binding_rows: HashSet<usize>,
    slots: HashMap<String, usize>,
    slot_names: Vec<String>,
    outcomes: Vec<LineOutcome>,
    values: Vec<Option<CalculationValue>>,
    work: script::WorkBudget,
    iterations: usize,
    iteration_limit: usize,
}

impl Execution {
    fn expression(
        &mut self,
        line: usize,
        path: &[usize],
    ) -> Result<(CalculationValue, Vec<usize>), Fault> {
        self.work.charge(1).map_err(|kind| Fault {
            line,
            kind,
            iteration: path.to_vec(),
        })?;
        if let Some(kind) = &self.parse_errors[line] {
            return Err(Fault {
                line,
                kind: kind.clone(),
                iteration: path.to_vec(),
            });
        }
        if let Some(name) = &self.compound_targets[line] {
            let Some(binding) = self.bindings.get(name) else {
                return Err(Fault {
                    line,
                    kind: DiagnosticKind::UnknownName(name.clone()),
                    iteration: path.to_vec(),
                });
            };
            if !matches!(binding.outcome, LineOutcome::Value(_)) {
                return Err(Fault {
                    line,
                    kind: DiagnosticKind::DependencyFailed { dependency_line: binding.line },
                    iteration: path.to_vec(),
                });
            }
        }
        if path.is_empty() && self.cyclic_candidates[line] {
            return Err(Fault {
                line,
                kind: DiagnosticKind::CyclicDependency,
                iteration: path.to_vec(),
            });
        }
        let Some(expression) = self.expressions[line].as_ref() else {
            let kind = match &self.rows[line].outcome {
                LineOutcome::Error(error) => error.kind.clone(),
                _ => DiagnosticKind::InvalidExpression,
            };
            return Err(Fault {
                line,
                kind,
                iteration: path.to_vec(),
            });
        };
        let mut dependencies = Vec::new();
        self.work.called_definitions.clear();
        self.work.runtime_line = Some(line);
        let result = expression.evaluate(
            line,
            &self.outcomes,
            &self.values,
            &mut dependencies,
            &mut self.work,
        );
        self.work.runtime_line = None;
        dependencies.extend(self.work.called_definitions.drain(..));
        let result = result.map_err(|kind| {
            let kind = match kind {
                DiagnosticKind::DependencyFailed { dependency_line }
                    if dependency_line >= SLOT_BASE =>
                {
                    DiagnosticKind::DependencyFailed {
                        dependency_line: self.bindings
                            [&self.slot_names[dependency_line - SLOT_BASE]]
                            .line,
                    }
                }
                other => other,
            };
            Fault {
                line,
                kind,
                iteration: path.to_vec(),
            }
        })?;
        dependencies = dependencies
            .into_iter()
            .filter_map(|dependency| {
                if dependency >= SLOT_BASE {
                    self.bindings
                        .get(&self.slot_names[dependency - SLOT_BASE])
                        .map(|binding| binding.line)
                } else {
                    Some(dependency)
                }
            })
            .collect();
        dependencies.sort_unstable();
        dependencies.dedup();
        Ok((result, dependencies))
    }

    fn publish(
        &mut self,
        line: usize,
        outcome: LineOutcome,
        dependencies: Vec<usize>,
        path: &[usize],
    ) {
        let row = &mut self.rows[line];
        row.outcome = outcome.clone();
        row.dependency_indices = dependencies;
        row.loop_iteration = (!path.is_empty()).then(|| path.to_vec());
        self.execution_counts[line] += 1;
        self.outcomes[line] = outcome.clone();
        self.values[line] = match &outcome {
            LineOutcome::Value(value) => Some(value.clone()),
            _ => None,
        };
        if let Some(name) = &row.assignment_name
            && self.binding_rows.contains(&line)
                && self.slots.contains_key(name)
                && !matches!(
                    &outcome,
                    LineOutcome::Error(Diagnostic {
                        kind: DiagnosticKind::ReservedName(_)
                    })
                )
            {
                if let Some(&slot) = self.slots.get(name) {
                    self.outcomes[slot] = outcome.clone();
                    self.values[slot] = match &outcome {
                        LineOutcome::Value(value) => Some(value.clone()),
                        _ => None,
                    };
                }
                self.bindings
                    .insert(name.clone(), Binding { line, outcome });
        }
    }

    fn execute(&mut self, statements: &[Statement], path: &mut Vec<usize>) -> Result<Flow, Fault> {
        for statement in statements {
            match statement {
                Statement::Definition => {}
                Statement::Flow { line, flow } => {
                    self.work.charge(1).map_err(|kind| Fault { line: *line, kind, iteration: path.clone() })?;
                    self.publish(*line, LineOutcome::Control, Vec::new(), path);
                    return Ok(*flow);
                }
                Statement::Conditional { branches } => {
                    let mut rows = Vec::new();
                    for branch in branches {
                        rows.push(branch.line);
                        affected_rows(&branch.body, &mut rows);
                    }
                    if let Some(&bad_line) = rows.iter().find(|&&row| self.parse_errors[row].is_some()) {
                        let fault = Fault {
                            line: bad_line,
                            kind: self.parse_errors[bad_line].clone().unwrap_or(DiagnosticKind::InvalidExpression),
                            iteration: path.clone(),
                        };
                        self.publish(bad_line, LineOutcome::Error(Diagnostic::new(fault.kind.clone())), Vec::new(), path);
                        if !path.is_empty() { return Err(fault); }
                        continue;
                    }
                    for branch in branches {
                        let selected = if branch.otherwise {
                            self.work.charge(1).map_err(|kind| Fault { line: branch.line, kind, iteration: path.clone() })?;
                            self.publish(branch.line, LineOutcome::Control, Vec::new(), path);
                            true
                        } else {
                            match self.expression(branch.line, path) {
                                Ok((CalculationValue::Boolean(value), dependencies)) => {
                                    self.publish(branch.line, LineOutcome::Value(CalculationValue::Boolean(value)), dependencies, path);
                                    value
                                }
                                Ok(_) => {
                                    let fault = Fault { line: branch.line, kind: DiagnosticKind::Script("conditional expression requires a Boolean value".into()), iteration: path.clone() };
                                    self.publish(branch.line, LineOutcome::Error(Diagnostic::new(fault.kind.clone())), Vec::new(), path);
                                    if !path.is_empty() { return Err(fault); }
                                    break;
                                }
                                Err(fault) => {
                                    self.publish(branch.line, LineOutcome::Error(Diagnostic::new(fault.kind.clone())), Vec::new(), path);
                                    if !path.is_empty() || matches!(fault.kind, DiagnosticKind::WorkLimit | DiagnosticKind::Canceled) { return Err(fault); }
                                    break;
                                }
                            }
                        };
                        if selected {
                            match self.execute(&branch.body, path)? {
                                Flow::Next => {}
                                flow => return Ok(flow),
                            }
                            break;
                        }
                    }
                }
                Statement::Line(line) => {
                    let result = self.expression(*line, path);
                    match result {
                        Ok((value, dependencies)) => {
                            self.publish(*line, LineOutcome::Value(value), dependencies, path)
                        }
                        Err(fault) => {
                            self.publish(
                                *line,
                                LineOutcome::Error(Diagnostic::new(fault.kind.clone())),
                                Vec::new(),
                                path,
                            );
                            if !path.is_empty()
                                || matches!(
                                    fault.kind,
                                    DiagnosticKind::WorkLimit | DiagnosticKind::Canceled
                                )
                            {
                                return Err(fault);
                            }
                        }
                    }
                }
                Statement::Loop { line, name, body } => {
                    let mut body_rows = Vec::new();
                    affected_rows(body, &mut body_rows);
                    if let Some(&bad_line) = body_rows
                        .iter()
                        .find(|&&row| self.parse_errors[row].is_some())
                    {
                        let fault = Fault {
                            line: bad_line,
                            kind: self.parse_errors[bad_line]
                                .clone()
                                .unwrap_or(DiagnosticKind::InvalidExpression),
                            iteration: path.clone(),
                        };
                        self.poison(*line, name, body, &fault, &[]);
                        if !path.is_empty() {
                            return Err(fault);
                        }
                        continue;
                    }
                    let (iterable, dependencies) = match self.expression(*line, path) {
                        Ok(result) => result,
                        Err(fault) => {
                            self.poison(*line, name, body, &fault, &[]);
                            if !path.is_empty()
                                || matches!(
                                    fault.kind,
                                    DiagnosticKind::WorkLimit | DiagnosticKind::Canceled
                                )
                            {
                                return Err(fault);
                            }
                            continue;
                        }
                    };
                    let values = match iterable {
                        CalculationValue::Vector(vector) => vector.components().to_vec(),
                        CalculationValue::Sequence(sequence) => sequence.values().to_vec(),
                        _ => {
                            let fault = Fault {
                                line: *line,
                                kind: DiagnosticKind::Script(
                                    "for iterable must be a scalar vector or sequence".into(),
                                ),
                                iteration: path.clone(),
                            };
                            self.poison(*line, name, body, &fault, &[]);
                            if !path.is_empty() {
                                return Err(fault);
                            }
                            continue;
                        }
                    };
                    let mut affected = vec![*line];
                    affected_rows(body, &mut affected);
                    if let Err(kind) = self.work.charge(affected.len()) {
                        let fault = Fault {
                            line: *line,
                            kind,
                            iteration: path.clone(),
                        };
                        self.poison(*line, name, body, &fault, &[]);
                        return Err(fault);
                    }
                    let old_rows = affected
                        .iter()
                        .map(|&index| (index, self.rows[index].clone()))
                        .collect::<Vec<_>>();
                    let old_counts = affected
                        .iter()
                        .map(|&index| (index, self.execution_counts[index]))
                        .collect::<Vec<_>>();
                    let old_bindings = affected
                        .iter()
                        .filter_map(|&index| {
                            self.rows[index]
                                .assignment_name
                                .as_ref()
                                .filter(|name| {
                                    self.binding_rows.contains(&index)
                                        && self.slots.contains_key(*name)
                                })
                                .map(|name| (name.clone(), self.bindings.get(name).cloned()))
                        })
                        .collect::<Vec<_>>();
                    for (index, value) in values.into_iter().enumerate() {
                        self.iterations += 1;
                        path.push(index + 1);
                        let result = if self.iterations > self.iteration_limit {
                            Err(Fault {
                                line: *line,
                                kind: DiagnosticKind::LoopIterationLimit,
                                iteration: path.clone(),
                            })
                        } else if let Err(kind) = self.work.charge(affected.len()) {
                            Err(Fault {
                                line: *line,
                                kind,
                                iteration: path.clone(),
                            })
                        } else {
                            for &body_line in affected.iter().skip(1) {
                                self.outcomes[body_line] = LineOutcome::Error(Diagnostic::new(
                                    DiagnosticKind::NotExecuted,
                                ));
                                self.values[body_line] = None;
                            }
                            self.publish(
                                *line,
                                LineOutcome::Value(CalculationValue::Scalar(value)),
                                dependencies.clone(),
                                path,
                            );
                            self.execute(body, path)
                        };
                        path.pop();
                        let break_requested = matches!(&result, Ok(Flow::Break));
                        if let Err(fault) = result {
                            let attempted = old_counts
                                .iter()
                                .filter_map(|&(index, count)| {
                                    (self.execution_counts[index] > count).then_some(index)
                                })
                                .collect::<Vec<_>>();
                            for (index, row) in old_rows {
                                self.outcomes[index] = row.outcome.clone();
                                self.values[index] = match &row.outcome {
                                    LineOutcome::Value(value) => Some(value.clone()),
                                    _ => None,
                                };
                                self.rows[index] = row;
                            }
                            for (name, binding) in old_bindings {
                                let slot = self.slots[&name];
                                match binding {
                                    Some(binding) => {
                                        self.outcomes[slot] = binding.outcome.clone();
                                        self.values[slot] = match &binding.outcome {
                                            LineOutcome::Value(value) => Some(value.clone()),
                                            _ => None,
                                        };
                                        self.bindings.insert(name, binding);
                                    }
                                    None => {
                                        self.outcomes[slot] = LineOutcome::Error(Diagnostic::new(
                                            DiagnosticKind::NotExecuted,
                                        ));
                                        self.values[slot] = None;
                                        self.bindings.remove(&name);
                                    }
                                }
                            }
                            self.poison(*line, name, body, &fault, &attempted);
                            for &index in &attempted {
                                if !self.binding_rows.contains(&index) && index != fault.line {
                                    let kind = DiagnosticKind::LoopError {
                                        iteration: fault.iteration.clone(),
                                        cause: Box::new(fault.kind.clone()),
                                    };
                                    self.publish(
                                        index,
                                        LineOutcome::Error(Diagnostic::new(kind)),
                                        Vec::new(),
                                        &fault.iteration,
                                    );
                                }
                            }
                            if !path.is_empty()
                                || matches!(
                                    fault.kind,
                                    DiagnosticKind::WorkLimit
                                        | DiagnosticKind::Canceled
                                        | DiagnosticKind::LoopIterationLimit
                                )
                            {
                                return Err(fault);
                            }
                            break;
                        }
                        if break_requested { break; }
                    }
                    for (index, count) in old_counts {
                        if path.is_empty() || self.execution_counts[index] > count {
                            self.outcomes[index] = self.rows[index].outcome.clone();
                            self.values[index] = match &self.rows[index].outcome {
                                LineOutcome::Value(value) => Some(value.clone()),
                                _ => None,
                            };
                        }
                    }
                }
            }
        }
        Ok(Flow::Next)
    }

    fn poison(
        &mut self,
        line: usize,
        name: &str,
        body: &[Statement],
        fault: &Fault,
        attempted: &[usize],
    ) {
        let error = DiagnosticKind::LoopError {
            iteration: fault.iteration.clone(),
            cause: Box::new(fault.kind.clone()),
        };
        self.rows[fault.line].outcome = LineOutcome::Error(Diagnostic::new(error.clone()));
        self.outcomes[fault.line] = self.rows[fault.line].outcome.clone();
        self.values[fault.line] = None;
        self.rows[fault.line].loop_iteration = Some(fault.iteration.clone());
        self.rows[line].assignment_name = Some(name.to_owned());
        self.publish(
            line,
            LineOutcome::Error(Diagnostic::new(error.clone())),
            Vec::new(),
            &fault.iteration,
        );
        let mut affected = Vec::new();
        affected_rows(body, &mut affected);
        for affected_line in affected {
            if self.binding_rows.contains(&affected_line) {
                let outcome = LineOutcome::Error(Diagnostic::new(error.clone()));
                if attempted.contains(&affected_line) {
                    self.publish(affected_line, outcome, Vec::new(), &fault.iteration);
                } else if let Some(name) = self.rows[affected_line].assignment_name.clone() {
                    let slot = self.slots[&name];
                    self.outcomes[slot] = outcome.clone();
                    self.values[slot] = None;
                    self.bindings.insert(
                        name,
                        Binding {
                            line: affected_line,
                            outcome,
                        },
                    );
                }
            }
        }
    }
}

pub(super) fn evaluate<'a>(
    source: &'a str,
    analysis: &syntax::SourceAnalysis,
    functions: &[Arc<script::UserFunction>],
    function_rows: &[Option<Result<(), DiagnosticKind>>],
    function_identifiers: usize,
    cancellation: Option<Arc<AtomicBool>>,
) -> CalculationEvaluation<'a> {
    evaluate_analyzed(source, analysis, functions, function_rows, function_identifiers, cancellation, MAX_LOOP_ITERATIONS)
}

#[cfg(test)]
fn evaluate_limited(
    source: &str,
    cancellation: Option<Arc<AtomicBool>>,
    iteration_limit: usize,
) -> CalculationEvaluation<'_> {
    let analysis = syntax::SourceAnalysis::new(source);
    let (functions, function_rows, function_identifiers) = script::build_functions(&analysis);
    evaluate_analyzed(source, &analysis, &functions, &function_rows, function_identifiers, cancellation, iteration_limit)
}

fn evaluate_analyzed<'a>(
    source: &'a str,
    analysis: &syntax::SourceAnalysis,
    functions: &[Arc<script::UserFunction>],
    function_rows: &[Option<Result<(), DiagnosticKind>>],
    function_identifiers: usize,
    cancellation: Option<Arc<AtomicBool>>,
    iteration_limit: usize,
) -> CalculationEvaluation<'a> {
    let lines = analysis.rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>();
    let headers = lines.iter().map(|line| header(line)).collect::<Vec<_>>();
    let mut declared = declared_names_in(analysis);
    for header in headers.iter().flatten().flatten() {
        declared.insert(header.name.clone());
    }
    let mut errors = vec![None; lines.len()];
    let mut count = function_identifiers;
    let mut canceled = false;
    let mut prepared = Vec::with_capacity(lines.len());
    let mut expressions = Vec::with_capacity(lines.len());
    let mut compound_targets = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let row = &analysis.rows[index];
        let end = row.offsets.iter().rev().find_map(|offset| *offset)
            .map_or(row.range.end, |offset| offset + 1);
        let range = row.range.start..end;
        if row.owner.is_some() {
            prepared.push(PreparedLine::simple(row.range.clone(), PreparedStatement::Blank));
            expressions.push(None);
            compound_targets.push(None);
            continue;
        }
        if function_rows[index].is_some() {
            prepared.push(PreparedLine::simple(row.range.clone(), PreparedStatement::Blank));
            expressions.push(None);
            compound_targets.push(None);
            if let Some(Err(error)) = &function_rows[index] { errors[index] = Some(error.clone()); }
            continue;
        }
        let branch = branch_header(line);
        if flow_statement(line).is_some() || matches!(&branch, Some(Ok(BranchHeader { kind: BranchKind::Else, .. }))) {
            prepared.push(PreparedLine::simple(row.range.clone(), PreparedStatement::Blank));
            expressions.push(None);
            compound_targets.push(None);
            continue;
        }
        if canceled
            || cancellation
                .as_ref()
                .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
        {
            canceled = true;
            prepared.push(PreparedLine::invalid(
                range,
                None,
                DiagnosticKind::NotExecuted,
            ));
            expressions.push(None);
            compound_targets.push(None);
            continue;
        }
        let text = line.trim();
        let expression = match &headers[index] {
            Some(Ok(header)) => header.expression.as_str(),
            _ => match &branch {
                Some(Ok(branch)) => branch.expression.as_deref().unwrap_or(text),
                _ => text,
            },
        };
        let visible = functions.partition_point(|function| function.line < index);
        let mut item = prepare_line_with_functions(range, expression, &mut count, &declared, &functions[..visible]);
        if let Some(diagnostic) = &row.diagnostic {
            item.diagnostic = Some(Diagnostic::new(diagnostic.clone()));
        }
        if let Some(Ok(header)) = &headers[index] {
            item.assignment_name = Some(header.name.clone());
            count = count.saturating_add(1);
            if count > MAX_DEPENDENCIES {
                item.diagnostic = Some(Diagnostic::new(DiagnosticKind::TooManyIdentifiers {
                    max_identifiers: MAX_DEPENDENCIES,
                }));
            }
        }
        if let Some(Err(error)) = &headers[index] {
            item.diagnostic = Some(Diagnostic::new(error.clone()));
        }
        let rhs = if headers[index].is_some() || branch.is_some() {
            expression
        } else {
            find_assignment_delimiter(text).map_or(text, |delimiter| text[delimiter.right_start..].trim())
        };
        let compiled = if item.diagnostic.is_some() || text.is_empty() || is_full_line_comment(text)
        {
            None
        } else {
            let statement = std::mem::replace(&mut item.statement, PreparedStatement::Invalid);
            match statement {
                PreparedStatement::Script(expression) => Some(*expression),
                statement => {
                    let references = item.references.clone();
                    let legacy = PreparedLine {
                        compound: None,
                        source_range: item.source_range.clone(),
                        assignment_name: None,
                        statement,
                        references,
                        dependencies: Vec::new(),
                        diagnostic: None,
                    };
                    Some(script::ScriptExpression::Legacy {
                        line: Box::new(legacy),
                        slots: Vec::new(),
                        alias: if item.compound.is_some() { None } else {
                            parse_simple_reference(rhs).map(|reference| reference.name)
                        },
                    })
                }
            }
        };
        compound_targets.push(item.compound.as_ref().and(item.assignment_name.clone()));
        expressions.push(compiled);
        prepared.push(item);
    }
    let mut index = 0;
    let statements = block(&lines, &headers, function_rows, &mut errors, &mut index, 0, 0, 0);
    let definitions = assignment_definitions(&prepared);
    let parse_errors = prepared
        .iter()
        .enumerate()
        .map(|(line, item)| {
            errors[line]
                .clone()
                .or_else(|| item.diagnostic.as_ref().map(|error| error.kind.clone()))
        })
        .collect::<Vec<_>>();
    let rows = prepared
        .iter()
        .enumerate()
        .map(|(line, item)| {
            let original = &source[analysis.rows[line].range.clone()];
            let outcome = if let Some(owner) = analysis.rows[line].owner {
                if original.trim().is_empty() { LineOutcome::Blank }
                else if is_full_line_comment(original.trim()) { LineOutcome::Comment }
                else { LineOutcome::Continuation { owner_line: owner } }
            } else if let Some(definition) = &function_rows[line] {
                match definition {
                    Ok(()) => LineOutcome::Definition,
                    Err(kind) => LineOutcome::Error(Diagnostic::new(kind.clone())),
                }
            } else if let Some(error) = errors[line]
                .as_ref()
                .map(|kind| Diagnostic::new(kind.clone()))
                .or_else(|| item.diagnostic.clone())
            {
                LineOutcome::Error(error)
            } else if is_full_line_comment(original.trim()) {
                LineOutcome::Comment
            } else if lines[line].trim().is_empty() {
                LineOutcome::Blank
            } else if is_full_line_comment(lines[line].trim()) {
                LineOutcome::Comment
            } else {
                LineOutcome::Error(Diagnostic::new(DiagnosticKind::NotExecuted))
            };
            CalculationLineResult {
                line_index: line,
                source_range: analysis.rows[line].range.clone(),
                assignment_name: item.assignment_name.clone(),
                dependency_indices: Vec::new(),
                loop_iteration: None,
                outcome,
            }
        })
        .collect::<Vec<_>>();
    resolve_references(&mut prepared, &definitions);
    let mut loop_rows = Vec::new();
    fn collect_loop_rows(statements: &[Statement], rows: &mut Vec<usize>) {
        for statement in statements {
            match statement {
                Statement::Loop { line, body, .. } => {
                    rows.push(*line);
                    affected_rows(body, rows);
                }
                Statement::Conditional { branches } => {
                    for branch in branches {
                        rows.push(branch.line);
                        affected_rows(&branch.body, rows);
                    }
                }
                _ => {}
            }
        }
    }
    collect_loop_rows(&statements, &mut loop_rows);
    for &line in &loop_rows {
        prepared[line].dependencies.clear();
    }
    for (line, target) in compound_targets.iter().enumerate() {
        let Some(name) = target else { continue; };
        let has_unconditional_prior = definitions.get(name).is_some_and(|locations| {
            locations.iter().any(|&prior| prior < line && !loop_rows.contains(&prior))
        });
        if !has_unconditional_prior {
            prepared[line].dependencies.clear();
        }
    }
    let cyclic_candidates = cyclic_lines(&prepared);
    let binding_rows = definitions
        .values()
        .flatten()
        .copied()
        .collect::<HashSet<_>>();
    let mut slot_names = definitions.keys().cloned().collect::<Vec<_>>();
    slot_names.sort();
    let slots = slot_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), SLOT_BASE + index))
        .collect::<HashMap<_, _>>();
    for (line, expression) in expressions.iter_mut().enumerate() {
        if let Some(expression) = expression {
            expression.resolve(line, rows.len(), |name| slots.get(name).copied());
        }
    }
    let mut outcomes = vec![
        LineOutcome::Error(Diagnostic::new(DiagnosticKind::NotExecuted));
        SLOT_BASE + slots.len()
    ];
    for row in &rows {
        outcomes[row.line_index] = row.outcome.clone();
    }
    let values = vec![None; outcomes.len()];
    let mut work = cancellation.map_or_else(
        script::WorkBudget::default,
        script::WorkBudget::with_cancellation,
    );
    work.runtime_definitions = Some(Arc::new(definitions));
    let mut execution = Execution {
        expressions,
        compound_targets,
        cyclic_candidates,
        parse_errors,
        execution_counts: vec![0; rows.len()],
        rows,
        bindings: HashMap::new(),
        binding_rows,
        slots,
        slot_names,
        outcomes,
        values,
        work,
        iterations: 0,
        iteration_limit,
    };
    let _ = execution.execute(&statements, &mut Vec::new());
    let final_bindings = execution
        .bindings
        .into_iter()
        .map(|(name, binding)| {
            (
                name,
                FinalBinding {
                    line_index: binding.line,
                    outcome: binding.outcome,
                },
            )
        })
        .collect();
    CalculationEvaluation {
        source,
        lines: execution.rows,
        final_bindings,
        diagnostic: None,
        usage: Some(EvaluationUsage { work_units: execution.work.used(), loop_iterations: execution.iterations }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(result: &CalculationEvaluation<'_>, line: usize) -> f64 {
        match &result.lines[line].outcome {
            LineOutcome::Value(CalculationValue::Scalar(value)) => *value,
            other => panic!("expected scalar at line {line}, got {other:?}"),
        }
    }

    #[test]
    fn conditional_branches_and_loop_control_run_lazily() {
        let source = concat!(
            "score = 85\n",
            "if score >= 80:\n",
            "  score += 10\n",
            "elif score > 0:\n",
            "  score = 1 / 0\n",
            "else:\n",
            "  score = 0\n",
            "score"
        );
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert_eq!(scalar(&result, 7), 95.0);
        assert!(matches!(result.lines[4].outcome, LineOutcome::Error(Diagnostic { kind: DiagnosticKind::NotExecuted })));

        let source = concat!(
            "first = 0\n",
            "for n in range(1, 15):\n",
            "  if n % 7 == 0:\n",
            "    first = n\n",
            "    break\n",
            "first"
        );
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert_eq!(scalar(&result, 5), 7.0);
        assert!(matches!(result.lines[4].outcome, LineOutcome::Control));

        let source = concat!(
            "total = 0\n",
            "for n in range(1, 11):\n",
            "  if n % 2 == 0:\n",
            "    continue\n",
            "  total += n\n",
            "total"
        );
        let document = CalculationDocument::new(source);
        assert_eq!(scalar(&document.evaluate(), 5), 25.0);
    }

    #[test]
    fn conditional_structure_contextual_names_and_failures_are_preserved() {
        let source = concat!(
            "if = 2\n",
            "break = 3\n",
            "break += 1\n",
            "if(false, 1 / 0, if + break)"
        );
        let document = CalculationDocument::new(source);
        assert_eq!(scalar(&document.evaluate(), 3), 6.0);

        let source = concat!(
            "base = 4\n",
            "if false:\n",
            "  if true:\n",
            "  misplaced = 5\n",
            "else:\n",
            "  base = 8\n",
            "base\n",
            "continue\n",
            "elif true:\n",
            "  1"
        );
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert!(result.lines.iter().any(|line| matches!(&line.outcome, LineOutcome::Error(Diagnostic { kind: DiagnosticKind::Script(message) }) if message.contains("indented body"))));
        assert!(matches!(result.lines[7].outcome, LineOutcome::Error(_)));
        assert!(matches!(result.lines[8].outcome, LineOutcome::Error(_)));

        let document = CalculationDocument::new("if 1:\n  x = 2\n3");
        assert!(matches!(document.evaluate().lines[0].outcome, LineOutcome::Error(_)));

        let document = CalculationDocument::new("x = 4\nif false:\n  x = 9\nx");
        assert_eq!(scalar(&document.evaluate(), 3), 4.0);

        let document = CalculationDocument::new("if false:\n  x = 2\nelif false:\n  x = 3\nelse:\n  x = 5\nx");
        let result = document.evaluate();
        assert_eq!(scalar(&result, 6), 5.0);
        assert!(matches!(result.lines[4].outcome, LineOutcome::Control));

        let document = CalculationDocument::new("if false:\n  only_skipped = 2\nonly_skipped");
        assert!(matches!(document.evaluate().lines[2].outcome, LineOutcome::Error(_)));
    }

    #[test]
    fn nested_loop_control_preserves_bindings_and_real_errors_roll_back() {
        let source = concat!(
            "count = 0\n",
            "for outer in range(3):\n",
            "  for inner in range(3):\n",
            "    if inner == 1:\n",
            "      break\n",
            "    count += 1\n",
            "count"
        );
        let document = CalculationDocument::new(source);
        assert_eq!(scalar(&document.evaluate(), 6), 3.0);

        let source = concat!(
            "total = 1\n",
            "for n in range(3):\n",
            "  total += 1\n",
            "  if n == 1:\n",
            "    bad = 1 / 0\n",
            "total"
        );
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert!(matches!(result.lines[1].outcome, LineOutcome::Error(_)));
        assert!(matches!(result.lines[5].outcome, LineOutcome::Error(_)));
    }

    #[test]
    fn conditional_function_calls_and_physical_control_rows_remain_distinct() {
        let source = concat!(
            "fn positive(x) = x > 0\r\n",
            "total = 0\r\n",
            "for n in range(1, 3):\r\n",
            "  if (positive(\r\n",
            "    n)):\r\n",
            "    total += n\r\n",
            "    break\r\n",
            "total\r\n",
            "r7"
        );
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert_eq!(result.source, source);
        assert_eq!(scalar(&result, 7), 1.0);
        assert!(matches!(result.lines[4].outcome, LineOutcome::Continuation { owner_line: 3 }));
        assert!(matches!(result.lines[6].outcome, LineOutcome::Control));
        assert!(matches!(result.lines[8].outcome, LineOutcome::Error(Diagnostic { kind: DiagnosticKind::ControlReference(_) })));
        assert!(result.lines[3].dependency_indices.contains(&0));
    }

    #[test]
    fn multiline_headers_and_loop_bodies_keep_physical_rows() {
        let source = concat!(
            "total = 0 # start\r\n",
            "for i in range(\r\n",
            "  1, 4): // header\r\n",
            "    total = (total +\r\n",
            "        i) # body\r\n",
            "total\r\n",
            "r5\r\n",
        );
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert_eq!(result.source, source);
        assert_eq!(scalar(&result, 5), 6.0);
        assert!(matches!(result.lines[2].outcome, LineOutcome::Continuation { owner_line: 1 }));
        assert!(matches!(result.lines[4].outcome, LineOutcome::Continuation { owner_line: 3 }));
        assert!(matches!(&result.lines[6].outcome, LineOutcome::Error(Diagnostic {
            kind: DiagnosticKind::ContinuationReference { owner_line: 3, .. },
        })));
    }

    #[test]
    fn loop_running_total_and_nested_bindings_use_latest_execution() {
        let document = CalculationDocument::new(concat!(
            "total = 0\n",
            "for amount in [10, 20, 30]:\n",
            "    doubled = amount * 2\n",
            "    total = total + doubled\n",
            "answer = total\n",
            "amount"
        ));
        let result = document.evaluate();
        assert_eq!(scalar(&result, 4), 120.0);
        assert_eq!(scalar(&result, 5), 30.0);
        assert_eq!(result.lines[3].loop_iteration, Some(vec![3]));
        assert!(result.lines[3].dependency_indices.contains(&3));

        let nested = CalculationDocument::new(concat!(
            "total = 0\n",
            "for x in [1, 2]:\n",
            "    for y in [3, 4]:\n",
            "        total = total + x * y\n",
            "total"
        ));
        let result = nested.evaluate();
        assert_eq!(scalar(&result, 4), 21.0);
        assert_eq!(result.lines[3].loop_iteration, Some(vec![2, 2]));
    }

    #[test]
    fn compound_assignments_use_each_iteration_binding_and_preserve_rows() {
        let source = "total = 0\r\nfor n in range(1, 4):\r\n    total += n\r\ntotal";
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert_eq!(scalar(&result, 0), 0.0);
        assert_eq!(scalar(&result, 2), 6.0);
        assert_eq!(scalar(&result, 3), 6.0);
        assert_eq!(result.lines[2].loop_iteration, Some(vec![3]));
        assert_eq!(result.lines[2].dependency_indices, [1, 2]);
        assert_eq!(&source[result.lines[2].source_range.clone()], "    total += n");
        assert_eq!(result.final_bindings["total"].line_index, 2);

        let nested = CalculationDocument::new(concat!(
            "{顧客 名} = 0\n",
            "for x in [1, 2]:\n",
            "    for y in [3, 4]:\n",
            "        {顧客 名} += x * y\n",
            "{顧客 名}"
        ));
        let result = nested.evaluate();
        assert_eq!(scalar(&result, 4), 21.0);
        assert_eq!(result.lines[3].loop_iteration, Some(vec![2, 2]));
        assert!(result.lines[3].dependency_indices.contains(&3));
    }

    #[test]
    fn compound_loop_guards_missing_and_failed_prior_before_rhs() {
        let missing = CalculationDocument::new(concat!(
            "missing += if(true, 1 / 0, 1)\n",
            "missing = 5\n",
            "for n in [1]:\n",
            "    later += 1\n",
            "later = 9"
        ));
        let result = missing.evaluate();
        assert!(matches!(&result.lines[0].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::UnknownName(name) }) if name == "missing"));
        assert!(matches!(&result.lines[3].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::LoopError { cause, .. } })
                if matches!(cause.as_ref(), DiagnosticKind::UnknownName(name) if name == "later")));

        let failed = CalculationDocument::new("x = 1 / 0\nfor n in [1]:\n    x += 1 / 0\nx");
        let result = failed.evaluate();
        assert!(matches!(&result.lines[2].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::LoopError { cause, .. } })
                if matches!(cause.as_ref(), DiagnosticKind::DependencyFailed { dependency_line: 0 })));
        assert!(matches!(&result.lines[3].outcome, LineOutcome::Error(_)));
    }

    #[test]
    fn compound_loop_document_preserves_real_cycles_and_prior_diagnostics() {
        let cyclic = CalculationDocument::new(concat!(
            "x = 1\nx += y\ny = x + 1\n",
            "for n in range(0):\n    n"
        ));
        let result = cyclic.evaluate();
        assert!(matches!(&result.lines[1].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::CyclicDependency })));
        let missing = CalculationDocument::new(concat!(
            "x += y\ny = x + 1\n",
            "for n in range(0):\n    n"
        ));
        let result = missing.evaluate();
        assert!(matches!(&result.lines[0].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::UnknownName(name) }) if name == "x"));
        assert!(matches!(&result.lines[1].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::DependencyFailed { dependency_line: 0 } })));

        let unpublished = CalculationDocument::new(concat!(
            "for n in range(0):\n    x = 1\n",
            "x += y\ny = x"
        ));
        let result = unpublished.evaluate();
        assert!(matches!(&result.lines[2].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::UnknownName(name) }) if name == "x"));
        assert!(matches!(&result.lines[3].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::DependencyFailed { dependency_line: 2 } })));

        let published = CalculationDocument::new(concat!(
            "for n in [1]:\n    x = 1\n",
            "x += y\ny = x"
        ));
        let result = published.evaluate();
        assert!(matches!(&result.lines[2].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::ForwardReference { dependency_line: 3, .. } })));
        assert!(matches!(&result.lines[3].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::DependencyFailed { dependency_line: 2 } })));
    }

    #[test]
    fn loop_header_rejects_compound_assignment() {
        let document = CalculationDocument::new(concat!(
            "n = 0\nx = [1, 2]\n",
            "for n in x += [3, 4]:\n    n\nx"
        ));
        let result = document.evaluate();
        assert!(matches!(&result.lines[2].outcome, LineOutcome::Error(_)));
        assert!(matches!(&result.lines[4].outcome, LineOutcome::Value(CalculationValue::Vector(vector))
            if vector.components() == [1.0, 2.0]));
    }

    #[test]
    fn compound_loop_rolls_back_failure_and_handles_empty_and_typed_values() {
        let failed = CalculationDocument::new(concat!(
            "total = 0\nfor divisor in [2, 0, 4]:\n",
            "    total += 10 / divisor\nafter = total\nindependent = 7"
        ));
        let result = failed.evaluate();
        assert_eq!(scalar(&result, 0), 0.0);
        assert!(matches!(&result.lines[2].outcome,
            LineOutcome::Error(Diagnostic { kind: DiagnosticKind::LoopError { iteration, .. } }) if iteration == &[2]));
        assert!(matches!(&result.lines[3].outcome, LineOutcome::Error(_)));
        assert_eq!(scalar(&result, 4), 7.0);

        let zero = CalculationDocument::new("x = 3\nfor n in range(0):\n    x += 1\nx");
        assert_eq!(scalar(&zero.evaluate(), 3), 3.0);
        let typed = CalculationDocument::new(concat!(
            "count = u64(9007199254740993)\n",
            "for n in [1, 2]:\n",
            "    count += 1\n",
            "count"
        ));
        let result = typed.evaluate();
        assert!(matches!(&result.lines[3].outcome, LineOutcome::Value(CalculationValue::Integer(value))
            if value.bit_pattern() == 9007199254740995));

        let fresh = CalculationDocument::new(concat!(
            "total = 0\nfor n in [1, 2]:\n",
            "    n += 2\n    total += if(n > 0, n, 1 / 0)\ntotal"
        ));
        assert_eq!(scalar(&fresh.evaluate(), 4), 7.0);
    }

    #[test]
    fn failed_and_empty_loops_preserve_transaction_boundaries() {
        let document = CalculationDocument::new(concat!(
            "total = 0\n",
            "for divisor in [2, 0, 4]:\n",
            "    total = total + 10 / divisor\n",
            "after = total\n",
            "independent = 7"
        ));
        let result = document.evaluate();
        assert_eq!(scalar(&result, 0), 0.0);
        assert!(
            matches!(&result.lines[2].outcome, LineOutcome::Error(Diagnostic { kind: DiagnosticKind::LoopError { iteration, .. } }) if iteration == &[2])
        );
        assert!(matches!(&result.lines[3].outcome, LineOutcome::Error(_)));
        assert_eq!(scalar(&result, 4), 7.0);

        let zero = CalculationDocument::new(concat!(
            "x = 3\n",
            "v = 9\n",
            "for v in range(0):\n",
            "    x = 5\n",
            "x\n",
            "v"
        ));
        let result = zero.evaluate();
        assert_eq!(scalar(&result, 4), 3.0);
        assert_eq!(scalar(&result, 5), 9.0);
        assert!(matches!(
            &result.lines[3].outcome,
            LineOutcome::Error(Diagnostic {
                kind: DiagnosticKind::NotExecuted
            })
        ));
    }

    #[test]
    fn loop_indentation_comments_unicode_and_physical_rows_are_preserved() {
        let document = CalculationDocument::new(concat!(
            "seed = 2\r\n",
            "for {name in group} in [1, 2]:\r\n",
            "    # comment\r\n",
            "    result = r1 + {name in group}\r\n",
            "result\r\n"
        ));
        let result = document.evaluate();
        assert_eq!(scalar(&result, 4), 4.0);
        assert_eq!(result.lines[3].dependency_indices, [0, 1]);
        let invalid = CalculationDocument::new("for x in [1]:\n\tx = 2\n");
        let result = invalid.evaluate();
        assert!(matches!(result.lines[0].outcome, LineOutcome::Error(_)));
        assert!(matches!(result.lines[1].outcome, LineOutcome::Error(_)));
        let legacy = CalculationDocument::new("\t2 + 3\n");
        assert_eq!(scalar(&legacy.evaluate(), 0), 5.0);
    }

    #[test]
    fn loop_work_limits_and_cancellation_are_explicit() {
        let document = CalculationDocument::new("for x in range(1025):\n    x\n7");
        let result = document.evaluate();
        assert!(matches!(result.lines[0].outcome, LineOutcome::Error(_)));
        assert_eq!(scalar(&result, 2), 7.0);

        let cancellation = Arc::new(AtomicBool::new(true));
        let result = document.evaluate_with_cancellation(cancellation);
        assert!(
            result
                .lines
                .iter()
                .any(|line| matches!(line.outcome, LineOutcome::Error(_)))
        );
        assert!(
            !result
                .lines
                .iter()
                .any(|line| matches!(line.outcome, LineOutcome::Value(_)))
        );
    }

    #[test]
    fn loop_validation_and_rollback_cover_unexecuted_and_expression_rows() {
        let zero = CalculationDocument::new(
            "for x in range(0):\n    bad = (\nafter = bad\nindependent = 7",
        );
        let result = zero.evaluate();
        assert!(matches!(result.lines[0].outcome, LineOutcome::Error(_)));
        assert!(matches!(result.lines[2].outcome, LineOutcome::Error(_)));
        assert_eq!(scalar(&result, 3), 7.0);

        let failed = CalculationDocument::new("for x in [1, 0]:\n    x * 2\n    10 / x\n    99");
        let result = failed.evaluate();
        assert!(matches!(
            result.lines[1].outcome,
            LineOutcome::Error(Diagnostic {
                kind: DiagnosticKind::LoopError { .. }
            })
        ));
        assert!(matches!(
            result.lines[2].outcome,
            LineOutcome::Error(Diagnostic {
                kind: DiagnosticKind::LoopError { .. }
            })
        ));
    }

    #[test]
    fn loop_physical_rows_restore_after_exit_but_are_fenced_within_iteration() {
        let source = "for n in [1, 0]:\n    for i in range(n):\n        v = 7\nr3";
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert_eq!(scalar(&result, 3), 7.0);

        let source = "for n in [1, 0]:\n    for i in range(n):\n        v = 7\n    r3";
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert!(matches!(result.lines[3].outcome, LineOutcome::Error(_)));

        let source = "for n in [1]:\n    for i in [1, 0]:\n        for j in range(i):\n            v = 7\n    after = r4\nafter";
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert_eq!(scalar(&result, 5), 7.0);
    }

    #[test]
    fn loop_contextual_names_do_not_change_existing_bindings() {
        let legacy = CalculationDocument::new("for = 8\nfor mod 3");
        assert!(!legacy.requires_repeated_execution());
        assert_eq!(scalar(&legacy.evaluate(), 1), 2.0);
        let legacy = CalculationDocument::new("for = u64(7)\nfor and u64(3)");
        assert!(!legacy.requires_repeated_execution());
        assert!(matches!(
            legacy.evaluate().lines[1].outcome,
            LineOutcome::Value(CalculationValue::Integer(value)) if value.bit_pattern() == 3
        ));

        let reserved = CalculationDocument::new("{sin} = 5\nfor x in [1]:\n    sin = 9\n{sin}");
        let result = reserved.evaluate();
        assert_eq!(scalar(&result, 3), 5.0);

        let forward =
            CalculationDocument::new("for x in [1]:\n    value = if({true}, 1, 0)\n{true} = true");
        let result = forward.evaluate();
        assert!(matches!(result.lines[1].outcome, LineOutcome::Error(_)));
    }

    #[test]
    fn loop_depth_iteration_and_space_limits_are_bounded() {
        fn nested(depth: usize) -> String {
            let mut source = String::new();
            for index in 0..depth {
                source.push_str(&format!("{}for x{index} in [1]:\n", "    ".repeat(index)));
            }
            source.push_str(&format!("{}7", "    ".repeat(depth)));
            source
        }
        let depth_eight = CalculationDocument::new(nested(8));
        assert_eq!(scalar(&depth_eight.evaluate(), 8), 7.0);
        let depth_nine = CalculationDocument::new(nested(9));
        assert!(
            depth_nine
                .evaluate()
                .lines
                .iter()
                .any(|row| matches!(row.outcome, LineOutcome::Error(_)))
        );
        let spaces = CalculationDocument::new("for x in [1]:\n\u{a0}x = 3");
        assert!(matches!(
            spaces.evaluate().lines[0].outcome,
            LineOutcome::Error(_)
        ));

        let source = "for x in [1, 2]:\n    for y in [1, 2]:\n        1\n8";
        let at_limit = evaluate_limited(source, None, 6);
        assert_eq!(scalar(&at_limit, 3), 8.0);
        let over_limit = evaluate_limited(source, None, 5);
        assert!(matches!(
            over_limit.lines[3].outcome,
            LineOutcome::Error(Diagnostic {
                kind: DiagnosticKind::NotExecuted
            })
        ));
    }

    #[test]
    fn non_loop_script_budget_stops_later_rows_and_bindings() {
        let mut source = String::new();
        for index in 0..30 {
            source.push_str(&format!("value{index} = sum(range(1024))\n"));
        }
        source.push_str("# still a comment\nlast = 7");
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert!(result.lines.iter().any(|row| matches!(
            row.outcome,
            LineOutcome::Error(Diagnostic {
                kind: DiagnosticKind::WorkLimit
            })
        )));
        assert!(matches!(result.lines[30].outcome, LineOutcome::Comment));
        assert!(matches!(
            result.lines[31].outcome,
            LineOutcome::Error(Diagnostic {
                kind: DiagnosticKind::NotExecuted
            })
        ));
        assert!(!result.final_bindings.contains_key("last"));
    }
    #[test]
    fn function_declarations_preserve_loop_structure_and_nested_rollback() {
        let source = concat!(
            "for i in range(2):\n",
            "fn identity(x) = x\n",
            "  stray = 3\n",
            "identity(4)"
        );
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert!(matches!(result.lines[0].outcome, LineOutcome::Error(_)));
        assert!(matches!(result.lines[1].outcome, LineOutcome::Definition));
        assert!(matches!(result.lines[2].outcome, LineOutcome::Error(_)));
        assert_eq!(scalar(&result, 3), 4.0);

        let source = concat!(
            "total = 1\n",
            "for i in range(2):\n",
            "  total += 1\n",
            "  fn invalid(x) = x\n",
            "total"
        );
        let document = CalculationDocument::new(source);
        let result = document.evaluate();
        assert!(matches!(result.lines[1].outcome, LineOutcome::Error(_)));
        assert!(matches!(result.lines[3].outcome, LineOutcome::Error(_)));
        assert!(matches!(result.lines[4].outcome, LineOutcome::Error(_)));
    }
}
