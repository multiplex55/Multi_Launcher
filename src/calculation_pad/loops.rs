//! Indentation-delimited, bounded loops over one document revision.

use super::*;
use std::sync::{Arc, atomic::AtomicBool};

const MAX_LOOP_DEPTH: usize = 8;
const MAX_LOOP_ITERATIONS: usize = 10_000;

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

pub(super) fn contains_loop(source: &str) -> bool {
    source.lines().any(|line| header(line).is_some())
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
    errors: &mut [Option<DiagnosticKind>],
    index: &mut usize,
    indent: usize,
    depth: usize,
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
        match &headers[line] {
            Some(Ok(header)) => {
                let next = next_statement(lines, *index);
                let body_indent = lines.get(next).and_then(|line| indentation(line).ok());
                if depth >= MAX_LOOP_DEPTH {
                    errors[line] = Some(DiagnosticKind::Script(
                        "loop nesting exceeds 8 levels".into(),
                    ));
                    statements.push(Statement::Line(line));
                } else if let Some(body_indent) = body_indent.filter(|value| *value > indent) {
                    let body = block(lines, headers, errors, index, body_indent, depth + 1);
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
        self.work.runtime_line = Some(line);
        let result = expression.evaluate(
            line,
            &self.outcomes,
            &self.values,
            &mut dependencies,
            &mut self.work,
        );
        self.work.runtime_line = None;
        let result = result.map_err(|kind| {
            let kind = match kind {
                DiagnosticKind::DependencyFailed { dependency_line }
                    if dependency_line >= MAX_SOURCE_LINES =>
                {
                    DiagnosticKind::DependencyFailed {
                        dependency_line: self.bindings
                            [&self.slot_names[dependency_line - MAX_SOURCE_LINES]]
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
                if dependency >= MAX_SOURCE_LINES {
                    self.bindings
                        .get(&self.slot_names[dependency - MAX_SOURCE_LINES])
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
        if let Some(name) = &row.assignment_name {
            if self.binding_rows.contains(&line)
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
    }

    fn execute(&mut self, statements: &[Statement], path: &mut Vec<usize>) -> Result<(), Fault> {
        for statement in statements {
            match statement {
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
        Ok(())
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

pub(super) fn evaluate(
    source: &str,
    cancellation: Option<Arc<AtomicBool>>,
) -> CalculationEvaluation<'_> {
    evaluate_limited(source, cancellation, MAX_LOOP_ITERATIONS)
}

fn evaluate_limited(
    source: &str,
    cancellation: Option<Arc<AtomicBool>>,
    iteration_limit: usize,
) -> CalculationEvaluation<'_> {
    let lines = source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect::<Vec<_>>();
    let headers = lines.iter().map(|line| header(line)).collect::<Vec<_>>();
    let mut declared = declared_names(source);
    for header in headers.iter().flatten().flatten() {
        declared.insert(header.name.clone());
    }
    let mut errors = vec![None; lines.len()];
    let mut offset = 0;
    let mut count = 0;
    let mut canceled = false;
    let mut prepared = Vec::with_capacity(lines.len());
    let mut expressions = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let range = offset..offset + line.len();
        let original = &source[range.clone()];
        offset += line.len()
            + usize::from(source.as_bytes().get(offset + line.len()) == Some(&b'\r'))
            + 1;
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
            continue;
        }
        let text = original.trim();
        let expression = match &headers[index] {
            Some(Ok(header)) => header.expression.as_str(),
            _ => text,
        };
        let mut item = prepare_line(range, expression, &mut count, &declared);
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
        let rhs = if headers[index].is_some() {
            expression
        } else {
            find_assignment_delimiter(text).map_or(text, |position| text[position + 1..].trim())
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
                        alias: parse_simple_reference(rhs).map(|reference| reference.name),
                    })
                }
            }
        };
        expressions.push(compiled);
        prepared.push(item);
    }
    let mut index = 0;
    let statements = block(&lines, &headers, &mut errors, &mut index, 0, 0);
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
            let outcome = if let Some(error) = errors[line]
                .as_ref()
                .map(|kind| Diagnostic::new(kind.clone()))
                .or_else(|| item.diagnostic.clone())
            {
                LineOutcome::Error(error)
            } else if lines[line].trim().is_empty() {
                LineOutcome::Blank
            } else if is_full_line_comment(lines[line].trim()) {
                LineOutcome::Comment
            } else {
                LineOutcome::Error(Diagnostic::new(DiagnosticKind::NotExecuted))
            };
            CalculationLineResult {
                line_index: line,
                source_range: item.source_range.clone(),
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
            if let Statement::Loop { line, body, .. } = statement {
                rows.push(*line);
                affected_rows(body, rows);
            }
        }
    }
    collect_loop_rows(&statements, &mut loop_rows);
    for &line in &loop_rows {
        prepared[line].dependencies.clear();
    }
    let mut parse_errors = parse_errors;
    let cyclic = cyclic_lines(&prepared);
    for (line, is_cyclic) in cyclic.into_iter().enumerate() {
        if is_cyclic && !loop_rows.contains(&line) && parse_errors[line].is_none() {
            // Keep legacy source-cycle diagnostics for ordinary top-level rows.
            parse_errors[line] = Some(DiagnosticKind::CyclicDependency);
        }
    }
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
        .map(|(index, name)| (name.clone(), MAX_SOURCE_LINES + index))
        .collect::<HashMap<_, _>>();
    for (line, expression) in expressions.iter_mut().enumerate() {
        if let Some(expression) = expression {
            expression.resolve(line, rows.len(), |name| slots.get(name).copied());
        }
    }
    let mut outcomes = vec![
        LineOutcome::Error(Diagnostic::new(DiagnosticKind::NotExecuted));
        MAX_SOURCE_LINES + slots.len()
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
}
