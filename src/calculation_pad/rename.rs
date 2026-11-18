//! Source-based rename planning. This module never evaluates the document.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RenameScope {
    Document,
    Parameter { owner_line: usize },
    Function { owner_line: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenamePlan {
    pub old_name: String,
    pub new_name: String,
    pub scope: RenameScope,
    pub ranges: Vec<Range<usize>>,
    pub lines: Vec<usize>,
    pub source: String,
}

fn select_token(tokens: &[Token], caret: usize) -> Result<&Token, String> {
    let selected = tokens.iter().find(|t| t.range.start <= caret && caret < t.range.end)
        .or_else(|| tokens.iter().find(|t| t.range.end == caret))
        .ok_or("Place the cursor on a variable or parameter.")?;
    if !tokens.iter().any(|t| t.scope == selected.scope && t.declaration && t.name == selected.name) {
        return Err("The selected name has no declaration in this scope.".into());
    }
    Ok(selected)
}

pub(crate) fn target(source: &str, caret: usize) -> Result<(String, RenameScope), String> {
    if !source.is_char_boundary(caret) { return Err("Invalid source position.".into()); }
    let tokens = tokens(source)?;
    let selected = select_token(&tokens, caret)?;
    Ok((selected.name.clone(), selected.scope.clone()))
}

#[derive(Clone)]
struct Token {
    name: String,
    range: Range<usize>,
    braced: bool,
    scope: RenameScope,
    declaration: bool,
}

fn infix_word(text: &str, start: usize, name: &str) -> bool {
    if !matches!(name, "and" | "or" | "xor" | "mod") {
        return false;
    }
    let before = text[..start].trim_end();
    let previous = before
        .rsplit(|c: char| {
            c.is_whitespace() || matches!(c, '(' | '[' | ',' | '+' | '-' | '*' | '/' | '%')
        })
        .next()
        .unwrap_or("");
    !matches!(previous, "and" | "or" | "xor" | "mod")
        && before
            .chars()
            .next_back()
            .is_some_and(|c| is_identifier_continue(c) || matches!(c, ')' | ']' | '}'))
}

fn original(row: &syntax::SourceRow, range: Range<usize>) -> Option<Range<usize>> {
    let offsets = row.offsets.get(range)?;
    let start = (*offsets.first()?)?;
    if offsets
        .iter()
        .enumerate()
        .all(|(i, offset)| *offset == Some(start + i))
    {
        Some(start..start + offsets.len())
    } else {
        None
    }
}

fn unit_spans(text: &str, base: usize, output: &mut Vec<Range<usize>>, depth: usize) {
    if depth > MAX_EXPRESSION_NESTING {
        return;
    }
    if parse_base_conversion(text).is_some_and(|result| result.is_ok()) {
        output.push(base..base + text.len());
        return;
    }
    let mut quantity_text = text;
    if let Some((left, target)) = split_to_keyword(text)
        && !left.is_empty()
        && (quantity::parse_unit_target(target).is_ok() || NumericBase::from_alias(target).is_some())
    {
        let target_start = target.as_ptr() as usize - text.as_ptr() as usize;
        let left_end = left.as_ptr() as usize - text.as_ptr() as usize + left.len();
        if let Some(offset) = text[left_end..target_start].to_ascii_lowercase().find("to") {
            output.push(base + left_end + offset..base + left_end + offset + 2);
        }
        output.push(base + target_start..base + target_start + target.len());
        quantity_text = left;
    }
    let start = quantity_text.as_ptr() as usize - text.as_ptr() as usize;
    output.extend(
        quantity::parse_expression(quantity_text)
            .unit_ranges
            .into_iter()
            .map(|range| base + start + range.start..base + start + range.end),
    );
    let mut cursor = 0;
    while cursor < text.len() {
        let ch = text[cursor..].chars().next().unwrap();
        if ch == '"' || ch == '{' {
            let close = if ch == '"' { '"' } else { '}' };
            cursor += ch.len_utf8();
            let mut escaped = false;
            while cursor < text.len() {
                let next = text[cursor..].chars().next().unwrap();
                cursor += next.len_utf8();
                if escaped {
                    escaped = false;
                } else if ch == '"' && next == '\\' {
                    escaped = true;
                } else if next == close {
                    break;
                }
            }
        } else if matches!(ch, '(' | '[') {
            let Ok(close) = script::matching(text, cursor) else {
                break;
            };
            let inner = &text[cursor + 1..close];
            if let Ok(arguments) = script::split_arguments(inner) {
                for argument in arguments {
                    let offset = argument.as_ptr() as usize - text.as_ptr() as usize;
                    unit_spans(argument, base + offset, output, depth + 1);
                }
            }
            cursor = close + 1;
        } else {
            cursor += ch.len_utf8();
        }
    }
}

#[derive(Default)]
pub(crate) struct InsertionFacts {
    pub names: Vec<(String, RenameScope, bool, Range<usize>)>,
    pub calls: Vec<(String, Range<usize>)>,
    pub physical_rows: Vec<(usize, Range<usize>)>,
}

pub(crate) fn insertion_facts(source: &str) -> Result<InsertionFacts, String> {
    let mut facts = InsertionFacts::default();
    let tokens = scan_tokens(source, Some(&mut facts))?;
    facts.names = tokens.into_iter().map(|token| (token.name, token.scope, token.declaration, token.range)).collect();
    Ok(facts)
}

fn tokens(source: &str) -> Result<Vec<Token>, String> {
    scan_tokens(source, None)
}

fn scan_tokens(source: &str, mut facts: Option<&mut InsertionFacts>) -> Result<Vec<Token>, String> {
    let analysis = syntax::SourceAnalysis::new(source);
    let (functions, function_rows, _) = script::build_functions(&analysis);
    let declared = declared_names_in(&analysis);
    let mut tokens = Vec::new();
    for (line, row) in analysis.rows.iter().enumerate() {
        if row.owner.is_some() {
            continue;
        }
        if row.diagnostic.is_some() {
            return Err(
                "Cannot rename while a statement has malformed grouping or quoting.".into(),
            );
        }
        let text = &row.text;
        let header = match script::declaration_header(text) {
            Some(Ok(header)) => Some(header),
            Some(Err(_)) => {
                return Err("Cannot rename while a function declaration is malformed.".into());
            }
            None => None,
        };
        if header.as_ref().is_some_and(|h| h.body_start.is_none()) {
            return Err("Cannot rename an incomplete function declaration.".into());
        }
        let iterator = loops::editor_iterator(text.trim());
        if text.trim_start().starts_with("for ")
            && text.trim_end().ends_with(':')
            && iterator.is_none()
        {
            return Err("Cannot rename while a loop header is malformed.".into());
        }
        let delimiter = if header.is_none() && iterator.is_none() {
            find_assignment_delimiter(text)
        } else {
            None
        };
        let assignment = delimiter
            .as_ref()
            .and_then(|d| parse_assignment_name(text[..d.left_end].trim()).ok());
        if delimiter.is_some() && assignment.is_none() {
            return Err("Cannot rename while an assignment is malformed.".into());
        }
        let control = loops::editor_control_keyword(text.trim());
        if function_rows[line].as_ref().is_some_and(Result::is_err) {
            return Err("Cannot rename while a function definition is invalid.".into());
        }
        if header.is_none() && iterator.is_none() && control.is_none() {
            let mut count = 0;
            let visible = functions.partition_point(|function| function.line < line);
            let prepared = prepare_line_with_functions(
                row.range.clone(),
                text,
                &mut count,
                &declared,
                &functions[..visible],
            );
            if prepared.diagnostic.is_some() {
                return Err("Cannot rename while a statement is malformed.".into());
            }
        }
        let leading = text.len() - text.trim_start().len();
        let expr_start = header
            .as_ref()
            .and_then(|h| h.body_start)
            .or_else(|| delimiter.as_ref().map(|d| d.right_start))
            .or_else(|| iterator.as_ref().and_then(|name| {
                let start = text[leading..].find(name)? + leading + name.len();
                let separator = text[start..].find(" in ")?;
                Some(start + separator + 4)
            }))
            .or_else(|| control.filter(|name| matches!(*name, "if" | "elif")).map(|name| leading + name.len()))
            .unwrap_or(0);
        let expr_end = if iterator.is_some() || matches!(control, Some("if" | "elif")) {
            text.trim_end().strip_suffix(':').map_or(text.len(), str::len)
        } else { text.len() };
        let expression = &text[expr_start..expr_end];
        let visible = functions.partition_point(|function| function.line < line);
        let callbacks = script::callback_ranges(expression, &functions[..visible])
            .into_iter()
            .map(|range| range.start + expr_start..range.end + expr_start)
            .collect::<Vec<_>>();
        let mut units = Vec::new();
        unit_spans(expression, expr_start, &mut units, 0);
        let mut pos = 0;
        while pos < text.len() {
            let ch = text[pos..].chars().next().ok_or("invalid source")?;
            if ch == '"' {
                pos += 1;
                let mut escaped = false;
                while pos < text.len() {
                    let c = text[pos..].chars().next().unwrap();
                    pos += c.len_utf8();
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        break;
                    }
                }
                continue;
            }
            let start = pos;
            let braced = ch == '{';
            if braced {
                pos += text[pos..].find('}').ok_or("Unclosed braced name")? + 1;
            } else if is_identifier_start(ch) {
                if text[..pos]
                    .chars()
                    .next_back()
                    .is_some_and(|c| is_identifier_continue(c) || c.is_ascii_digit() || c == '.')
                {
                    pos += ch.len_utf8();
                    continue;
                }
                pos += ch.len_utf8();
                while pos < text.len() {
                    let c = text[pos..].chars().next().unwrap();
                    if !is_identifier_continue(c) {
                        break;
                    }
                    pos += c.len_utf8();
                }
            } else {
                pos += ch.len_utf8();
                continue;
            }
            let authored = &text[start..pos];
            let parsed = parse_assignment_name(authored).map_err(|_| "Invalid name in source")?;
            if let Some(header) = &header
                && header.name_range == (start..pos)
            {
                let range = original(row, start..pos).ok_or("Name crosses a logical separator")?;
                tokens.push(Token { name: parsed.canonical, range, braced: false,
                    scope: RenameScope::Function { owner_line: line }, declaration: true });
                continue;
            }
            if !braced && start >= expr_start && pos <= expr_end
                && (text[pos..].trim_start().starts_with('(') || callbacks.contains(&(start..pos)))
                && let Some(facts) = facts.as_deref_mut()
                && let Some(range) = original(row, start..pos)
            {
                facts.calls.push((parsed.canonical.clone(), range));
            }
            if !braced && start >= expr_start && pos <= expr_end
                && (text[pos..].trim_start().starts_with('(') || callbacks.contains(&(start..pos)))
                && let Some(function) = functions[..visible].iter().find(|function| function.name == authored)
            {
                let range = original(row, start..pos).ok_or("Name crosses a logical separator")?;
                tokens.push(Token { name: parsed.canonical, range, braced: false,
                    scope: RenameScope::Function { owner_line: function.line }, declaration: false });
                continue;
            }
            if !braced && callbacks.contains(&(start..pos)) {
                continue;
            }
            if is_row_reference_name(&parsed.canonical) {
                if (braced || !text[pos..].trim_start().starts_with('('))
                    && let Some(facts) = facts.as_deref_mut()
                    && let RowReference::Line(index) = parse_row_reference(&parsed.canonical)
                    && let Some(range) = original(row, start..pos)
                {
                    facts.physical_rows.push((index + 1, range));
                }
                continue;
            }
            let mut scope = RenameScope::Document;
            let declaration;
            if let Some(header) = &header {
                if let Some(parameter) = header.parameters.iter().find(|p| p.range == (start..pos))
                {
                    scope = RenameScope::Parameter { owner_line: line };
                    declaration = true;
                    debug_assert_eq!(parameter.canonical, parsed.canonical);
                } else if header.body_start.is_some_and(|body| start >= body) {
                    if !braced
                        && (text[pos..].trim_start().starts_with('(')
                            || is_reserved_name(authored)
                            || matches!(authored, "true" | "false")
                            || infix_word(text, start, authored)
                            || units
                                .iter()
                                .any(|range| range.start <= start && pos <= range.end))
                    {
                        continue;
                    }
                    scope = RenameScope::Parameter { owner_line: line };
                    declaration = false;
                } else {
                    continue;
                }
            } else {
                declaration = assignment.as_ref().is_some_and(|a| {
                    a.canonical == parsed.canonical
                        && delimiter.as_ref().is_some_and(|d| pos <= d.left_end)
                }) || iterator
                    .as_ref()
                    .and_then(|name| parse_assignment_name(name).ok())
                    .is_some_and(|name| {
                        name.canonical == parsed.canonical && text[..start].trim() == "for"
                    });
                if !declaration {
                    if !braced
                        && ((start == leading && control.is_some())
                            || (iterator.is_some() && ((start == leading && authored == "for") || (authored == "in" && start + 3 == expr_start)))
                            || text[pos..].trim_start().starts_with('('))
                    {
                        continue;
                    }
                    if !braced
                        && (is_reserved_name(authored)
                            || matches!(authored, "true" | "false")
                            || infix_word(text, start, authored))
                    {
                        continue;
                    }
                    if !braced
                        && units
                            .iter()
                            .any(|range| range.start <= start && pos <= range.end)
                    {
                        continue;
                    }
                }
            }
            let range = original(row, start..pos).ok_or("Name crosses a logical separator")?;
            tokens.push(Token {
                name: parsed.canonical,
                range,
                braced,
                scope,
                declaration,
            });
        }
    }
    Ok(tokens)
}

pub(crate) fn plan(source: &str, caret: usize, replacement: &str) -> Result<RenamePlan, String> {
    if !source.is_char_boundary(caret) {
        return Err("Invalid source position.".into());
    }
    let parsed = parse_assignment_name(replacement).map_err(|_| "Enter a valid variable name.")?;
    if is_row_reference_name(&parsed.canonical)
        || (!parsed.braced && is_reserved_name(&parsed.canonical))
    {
        return Err("That name is reserved.".into());
    }
    let tokens = tokens(source)?;
    let selected = select_token(&tokens, caret)?;
    if matches!(selected.scope, RenameScope::Function { .. })
        && (parsed.braced || script::signature(&parsed.canonical).is_some())
    {
        return Err("Enter an unused bare function name.".into());
    }
    if selected.name == parsed.canonical {
        return Err("Choose a different name.".into());
    }
    if tokens
        .iter()
        .any(|t| t.scope == selected.scope && t.name == parsed.canonical)
    {
        return Err("The new name conflicts with an existing binding or reference.".into());
    }
    let targets = tokens
        .iter()
        .filter(|t| t.scope == selected.scope && t.name == selected.name)
        .collect::<Vec<_>>();
    let requires_braces = parsed.braced || parsed.canonical.eq_ignore_ascii_case("to");
    let output_len = targets.iter().try_fold(source.len(), |length, token| {
        let replacement_len = parsed.canonical.len() + usize::from(token.braced || requires_braces) * 2;
        length.checked_sub(token.range.len())?.checked_add(replacement_len)
    }).ok_or("The renamed document exceeds the source limit.")?;
    if output_len > super::super::limits::ABSOLUTE_BYTES { return Err("The renamed document exceeds the source limit.".into()); }
    let mut output = String::with_capacity(output_len);
    let mut cursor = 0;
    let mut lines = Vec::new();
    let mut line = 0;
    for token in &targets {
        let unchanged = &source[cursor..token.range.start];
        line += unchanged.bytes().filter(|b| *b == b'\n').count();
        if lines.last().copied() != Some(line) {
            lines.push(line);
        }
        output.push_str(unchanged);
        let rendered = if token.braced || requires_braces {
            format!("{{{}}}", parsed.canonical)
        } else {
            parsed.canonical.clone()
        };
        output.push_str(&rendered);
        cursor = token.range.end;
    }
    output.push_str(&source[cursor..]);
    let projected = self::tokens(&output)?;
    if projected.len() != tokens.len()
        || !tokens.iter().zip(&projected).all(|(before, after)| {
            let name = if before.scope == selected.scope && before.name == selected.name {
                &parsed.canonical
            } else {
                &before.name
            };
            after.name == *name
                && after.scope == before.scope
                && after.declaration == before.declaration
        })
    {
        return Err("The new name would change how the expression is parsed.".into());
    }
    let ranges = targets.iter().map(|t| t.range.clone()).collect::<Vec<_>>();
    Ok(RenamePlan {
        old_name: selected.name.clone(),
        new_name: parsed.canonical,
        scope: selected.scope.clone(),
        ranges,
        lines,
        source: output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_rename_tracks_calls_and_callbacks_without_capturing_values() {
        let source = concat!(
            "scale = 3\r\nfn scale(value, factor) = value * factor\r\n",
            "values = [1, 2]\r\nmap(\r\n values, scale, scale\r\n)\r\n",
            "scale(2, 3)\r\nscale # scale\r\n"
        );
        let caret = source.find("fn scale").unwrap() + 3;
        let preview = plan(source, caret, "multiply").unwrap();
        assert_eq!(preview.scope, RenameScope::Function { owner_line: 1 });
        assert!(preview.source.contains("fn multiply(value, factor)"));
        assert!(preview.source.contains("values, multiply, scale"));
        assert!(preview.source.contains("multiply(2, 3)"));
        assert!(preview.source.ends_with("scale # scale\r\n"));
        assert!(plan(source, caret, "{other}").is_err());
        assert!(plan(source, caret, "sum").is_err());
        let variable = plan(source, 0, "amount").unwrap();
        assert!(variable.source.contains("values, scale, amount"));
        assert!(variable.source.contains("fn scale(value, factor)"));
        let parameter = source.find("value, factor").unwrap();
        let parameter_plan = plan(source, parameter, "item").unwrap();
        assert!(parameter_plan.source.contains("fn scale(item, factor) = item * factor"));
        assert!(parameter_plan.source.contains("values, scale, scale"));
    }

    #[test]
    fn row_named_formula_is_distinct_from_physical_row_reference() {
        let source = "1\n2\n3\nfn r3(x) = x * 2\nmap([1], r3)\nr3(4)\nr3\n";
        let caret = source.find("fn r3").unwrap() + 3;
        let preview = plan(source, caret, "double_value").unwrap();
        assert_eq!(preview.source, "1\n2\n3\nfn double_value(x) = x * 2\nmap([1], double_value)\ndouble_value(4)\nr3\n");
    }

    #[test]
    fn unresolved_callback_is_not_a_document_value_reference() {
        let source = "x = 2\nmap([1], x)\nx";
        let preview = plan(source, 0, "value").unwrap();
        assert_eq!(preview.source, "value = 2\nmap([1], x)\nvalue");
    }

    #[test]
    fn document_rename_preserves_units_keywords_comments_and_physical_rows() {
        let source = "s = 2\r\nd = 10 m\r\nd / s\r\n1 m/s to km/h # s\r\nfor s in range(2):\r\n  s += 1\r\nfn f(s) = s + 1\r\n{gross s} = s";
        let preview = plan(source, 0, "speed_scale").unwrap();
        assert_eq!(
            preview.source,
            "speed_scale = 2\r\nd = 10 m\r\nd / speed_scale\r\n1 m/s to km/h # s\r\nfor speed_scale in range(2):\r\n  speed_scale += 1\r\nfn f(s) = s + 1\r\n{gross s} = speed_scale"
        );
        assert_eq!(preview.lines, [0, 2, 4, 5, 7]);
        let branch = "s = 2\nif false:\n  if 1 s < s:\n    s += 1";
        assert_eq!(plan(branch, 0, "amount").unwrap().source, "amount = 2\nif false:\n  if 1 s < amount:\n    amount += 1");
        let mod_source = "mod = 5\nmod + 1\n7 mod mod";
        assert_eq!(
            plan(mod_source, 0, "amount").unwrap().source,
            "amount = 5\namount + 1\n7 mod amount"
        );
    }

    #[test]
    fn parameter_scope_capture_and_malformed_sources_are_checked() {
        let source = "x = 1\nfn one({x}, y) = ({x} +\r\n y)\nfn two(x) = x\nx";
        let offset = source.find("{x}").unwrap();
        let preview = plan(source, offset, "value").unwrap();
        assert_eq!(preview.scope, RenameScope::Parameter { owner_line: 1 });
        assert_eq!(
            preview.source,
            "x = 1\nfn one({value}, y) = ({value} +\r\n y)\nfn two(x) = x\nx"
        );
        assert!(plan(source, offset, "y").is_err());
        assert!(plan("count = 1\nmissing + count", 0, "missing").is_err());
        assert!(plan("count = (1\ncount", 0, "total").is_err());
    }

    #[test]
    fn contextual_to_in_and_for_preserve_variables_and_grammar() {
        let source = "to = 2\nin = 3\nfor = 4\nconverted = 1 m to cm\nfor i in [in]:\n  y = to*1 + in + for + i\nto+1";
        let to = plan(source, 0, "distance").unwrap();
        assert_eq!(to.source, "distance = 2\nin = 3\nfor = 4\nconverted = 1 m to cm\nfor i in [in]:\n  y = distance*1 + in + for + i\ndistance+1");
        let input = plan(source, source.find("in =").unwrap(), "amount").unwrap();
        assert_eq!(input.source, "to = 2\namount = 3\nfor = 4\nconverted = 1 m to cm\nfor i in [amount]:\n  y = to*1 + amount + for + i\nto+1");
        let for_var = plan(source, source.find("for =").unwrap(), "other").unwrap();
        assert_eq!(for_var.source, "to = 2\nin = 3\nother = 4\nconverted = 1 m to cm\nfor i in [in]:\n  y = to*1 + in + other + i\nto+1");
        for text in [source, to.source.as_str(), input.source.as_str(), for_var.source.as_str()] {
            let document = CalculationDocument::new(text);
            let evaluation = document.evaluate();
            assert!(matches!(evaluation.lines[5].outcome, LineOutcome::Value(CalculationValue::Scalar(12.0))), "{:?}", evaluation.lines[5].outcome);
            assert!(matches!(evaluation.lines[6].outcome, LineOutcome::Value(CalculationValue::Scalar(3.0))), "{:?}", evaluation.lines[6].outcome);
        }
    }

    #[test]
    fn contextual_base_conversion_preserves_authored_radix_tokens() {
        let source = "to = 2\nhex = 7\ndec = 8\nff hex to dec\nhex+dec+to";
        for (name, replacement) in [("to", "travel"), ("hex", "radix_var"), ("dec", "decimal_var")] {
            let offset = source.find(&format!("{name} =")).unwrap();
            let preview = plan(source, offset, replacement).unwrap();
            assert!(preview.source.contains("ff hex to dec"));
            let document = CalculationDocument::new(preview.source);
            let evaluation = document.evaluate();
            assert!(matches!(evaluation.lines[3].outcome, LineOutcome::Value(CalculationValue::ExactBase { .. })));
            assert!(matches!(evaluation.lines[4].outcome, LineOutcome::Value(CalculationValue::Scalar(17.0))));
        }
        let referenced = "x = 0xff\nhex = 7\nx to hex\nhex+1";
        assert_eq!(plan(referenced, referenced.find("hex =").unwrap(), "radix_var").unwrap().source,
            "x = 0xff\nradix_var = 7\nx to hex\nradix_var+1");
    }

    #[test]
    fn rename_to_conversion_keyword_braces_destination_and_preserves_values() {
        let source = "count = 1\ncount + 1";
        for destination in ["to", "TO"] {
            let preview = plan(source, 0, destination).unwrap();
            assert_eq!(preview.new_name, destination);
            assert_eq!(preview.source, format!("{{{destination}}} = 1\n{{{destination}}} + 1"));
            for text in [source, preview.source.as_str()] {
                let document = CalculationDocument::new(text);
                let evaluation = document.evaluate();
                assert!(matches!(evaluation.lines[0].outcome, LineOutcome::Value(CalculationValue::Scalar(1.0))));
                assert!(matches!(evaluation.lines[1].outcome, LineOutcome::Value(CalculationValue::Scalar(2.0))));
            }
        }
    }
}
