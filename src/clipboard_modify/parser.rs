use super::catalog::{ArgumentRequirements, normalize_name, operation_lookup, operations};
use super::model::{ClipboardModifierCatalog, OperationId, StageArguments, StageSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardModifyParseResult {
    NotClipboardModify,
    OpenSection(ModifySection),
    CompleteExecution(ClipboardModifyIntent),
    Partial(PartialQuery),
    Invalid(ParserError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifySection {
    Modify,
    Templates,
    SavedPipelines,
    ManageTemplates,
    ManagePipelines,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardModifyIntent {
    Stages(Vec<StageSpec>),
    ApplyTemplate { name: String },
    ApplySavedPipeline { name: String },
    Undo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialQuery {
    pub stage_index: usize,
    pub section: ModifySection,
    pub query: String,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserError {
    pub stage_index: Option<usize>,
    pub span: Span,
    pub kind: ParserErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParserErrorKind {
    UnterminatedQuote,
    TrailingEscape,
    LeadingPipe,
    TrailingPipe,
    EmptyStage,
    UnknownCommand(String),
    MissingArgument {
        operation: String,
        argument: &'static str,
    },
    UnexpectedArgument {
        operation: String,
        argument: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    span: Span,
    quoted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stage {
    tokens: Vec<Token>,
    span: Span,
}

pub fn parse(input: &str, catalog: &ClipboardModifierCatalog) -> ClipboardModifyParseResult {
    let (prefix, rest_start) = match cm_prefix(input) {
        Some(v) => v,
        None => return ClipboardModifyParseResult::NotClipboardModify,
    };
    if input[prefix.end..].trim().is_empty() {
        return ClipboardModifyParseResult::OpenSection(ModifySection::Modify);
    }
    let rest = &input[rest_start..];
    let stages = match lex_stages(rest, rest_start) {
        Ok(s) => s,
        Err(e) => return ClipboardModifyParseResult::Invalid(e),
    };
    if stages.is_empty() {
        return ClipboardModifyParseResult::OpenSection(ModifySection::Modify);
    }
    if stages.len() == 1
        && let Some(special) = parse_special(&stages[0], catalog)
    {
        return special;
    }
    let mut out = Vec::new();
    for (idx, stage) in stages.iter().enumerate() {
        match parse_stage(stage, idx) {
            Ok(spec) => out.push(spec),
            Err(e) => return e,
        }
    }
    ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::Stages(out))
}

fn cm_prefix(input: &str) -> Option<(Span, usize)> {
    let trimmed = input.trim_start();
    let start = input.len() - trimmed.len();
    let mut it = trimmed.char_indices();
    let (_, c) = it.next()?;
    if !c.eq_ignore_ascii_case(&'c') {
        return None;
    }
    let (_, m) = it.next()?;
    if !m.eq_ignore_ascii_case(&'m') {
        return None;
    }
    let after = it.next().map(|(i, _)| i).unwrap_or(trimmed.len());
    if trimmed[after..]
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace())
    {
        return None;
    }
    Some((
        Span {
            start,
            end: start + after,
        },
        start + after,
    ))
}

fn lex_stages(input: &str, offset: usize) -> Result<Vec<Stage>, ParserError> {
    let mut stages = vec![Stage {
        tokens: vec![],
        span: Span {
            start: offset,
            end: offset,
        },
    }];
    let mut chars = input.char_indices().peekable();
    while let Some((i, ch)) = chars.peek().copied() {
        let abs = offset + i;
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '|' {
            chars.next();
            let stage_idx = stages.len() - 1;
            let cur_empty = stages[stage_idx].tokens.is_empty();
            stages[stage_idx].span.end = abs;
            if stage_idx == 0 && cur_empty {
                return Err(err(None, abs, abs + 1, ParserErrorKind::LeadingPipe));
            }
            if cur_empty {
                return Err(err(
                    Some(stage_idx),
                    abs,
                    abs + 1,
                    ParserErrorKind::EmptyStage,
                ));
            }
            stages.push(Stage {
                tokens: vec![],
                span: Span {
                    start: abs + 1,
                    end: abs + 1,
                },
            });
            continue;
        }
        let quoted = ch == '"' || ch == '\'';
        let tok = if quoted {
            lex_quoted(&mut chars, offset)?
        } else {
            lex_bare(&mut chars, offset)?
        };
        let cur = stages.last_mut().unwrap();
        if cur.tokens.is_empty() {
            cur.span.start = tok.span.start;
        }
        cur.span.end = tok.span.end;
        cur.tokens.push(tok);
    }
    if let Some(last) = stages.last()
        && last.tokens.is_empty()
        && stages.len() > 1
    {
        return Err(err(
            Some(stages.len() - 1),
            last.span.start.saturating_sub(1),
            last.span.start,
            ParserErrorKind::TrailingPipe,
        ));
    }
    Ok(stages)
}
fn lex_bare<I: Iterator<Item = (usize, char)>>(
    chars: &mut std::iter::Peekable<I>,
    offset: usize,
) -> Result<Token, ParserError> {
    let (start_i, _) = chars.peek().copied().unwrap();
    let mut text = String::new();
    let mut end = offset + start_i;
    while let Some((i, ch)) = chars.peek().copied() {
        if ch.is_whitespace() || ch == '|' {
            break;
        }
        chars.next();
        if ch == '\\' {
            if let Some((j, n)) = chars.next() {
                text.push(n);
                end = offset + j + n.len_utf8();
            } else {
                return Err(err(
                    None,
                    offset + i,
                    offset + i + 1,
                    ParserErrorKind::TrailingEscape,
                ));
            }
        } else {
            text.push(ch);
            end = offset + i + ch.len_utf8();
        }
    }
    Ok(Token {
        text,
        span: Span {
            start: offset + start_i,
            end,
        },
        quoted: false,
    })
}
fn lex_quoted<I: Iterator<Item = (usize, char)>>(
    chars: &mut std::iter::Peekable<I>,
    offset: usize,
) -> Result<Token, ParserError> {
    let (start_i, quote) = chars.next().unwrap();
    let mut text = String::new();
    let mut end = offset + start_i + quote.len_utf8();
    while let Some((i, ch)) = chars.next() {
        if ch == quote {
            end = offset + i + ch.len_utf8();
            return Ok(Token {
                text,
                span: Span {
                    start: offset + start_i,
                    end,
                },
                quoted: true,
            });
        }
        if ch == '\\' {
            if let Some((j, n)) = chars.next() {
                text.push(n);
                end = offset + j + n.len_utf8();
            } else {
                return Err(err(
                    None,
                    offset + i,
                    offset + i + 1,
                    ParserErrorKind::TrailingEscape,
                ));
            }
        } else {
            text.push(ch);
            end = offset + i + ch.len_utf8();
        }
    }
    Err(err(
        None,
        offset + start_i,
        end,
        ParserErrorKind::UnterminatedQuote,
    ))
}

fn parse_special(
    stage: &Stage,
    catalog: &ClipboardModifierCatalog,
) -> Option<ClipboardModifyParseResult> {
    let first = stage.tokens.first()?;
    let n = normalize_name(&first.text);
    if stage.tokens.len() == 1 {
        match n.as_str() {
            "modify" => {
                return Some(ClipboardModifyParseResult::OpenSection(
                    ModifySection::Modify,
                ));
            }
            "manage-templates" => {
                return Some(ClipboardModifyParseResult::OpenSection(
                    ModifySection::ManageTemplates,
                ));
            }
            "manage-pipelines" => {
                return Some(ClipboardModifyParseResult::OpenSection(
                    ModifySection::ManagePipelines,
                ));
            }
            "help" => return Some(ClipboardModifyParseResult::OpenSection(ModifySection::Help)),
            "undo" => {
                return Some(ClipboardModifyParseResult::CompleteExecution(
                    ClipboardModifyIntent::Undo,
                ));
            }
            _ => {}
        }
    }
    if n == "template" {
        return Some(if stage.tokens.len() == 1 {
            ClipboardModifyParseResult::OpenSection(ModifySection::Templates)
        } else {
            let q = join_tokens(&stage.tokens[1..]);
            if is_prefix_like(stage) {
                partial(
                    0,
                    ModifySection::Templates,
                    q,
                    catalog
                        .templates
                        .iter()
                        .flat_map(|t| std::iter::once(&t.id).chain(t.aliases.iter()))
                        .cloned()
                        .collect(),
                )
            } else {
                ClipboardModifyParseResult::CompleteExecution(
                    ClipboardModifyIntent::ApplyTemplate { name: q },
                )
            }
        });
    }
    if n == "apply" {
        return Some(if stage.tokens.len() == 1 {
            ClipboardModifyParseResult::OpenSection(ModifySection::SavedPipelines)
        } else {
            let q = join_tokens(&stage.tokens[1..]);
            if is_prefix_like(stage) {
                partial(
                    0,
                    ModifySection::SavedPipelines,
                    q,
                    catalog
                        .pipelines
                        .iter()
                        .flat_map(|p| std::iter::once(&p.id).chain(p.aliases.iter()))
                        .cloned()
                        .collect(),
                )
            } else {
                ClipboardModifyParseResult::CompleteExecution(
                    ClipboardModifyIntent::ApplySavedPipeline { name: q },
                )
            }
        });
    }
    if n == "wrap" && stage.tokens.len() == 1 {
        return Some(partial(
            0,
            ModifySection::Modify,
            "wrap".into(),
            vec!["wrap quotes".into(), "wrap <prefix> <suffix>".into()],
        ));
    }
    if stage.tokens.len() == 1 && !stage.tokens[0].quoted {
        let q = normalize_name(&stage.tokens[0].text);
        if operation_lookup(&q).is_none() {
            let sug = suggestions(&q);
            if !sug.is_empty() {
                return Some(partial(0, ModifySection::Modify, q, sug));
            }
        }
    }
    None
}
fn is_prefix_like(stage: &Stage) -> bool {
    stage.tokens.len() == 2 && !stage.tokens[1].quoted
}
fn partial(
    stage_index: usize,
    section: ModifySection,
    query: String,
    all: Vec<String>,
) -> ClipboardModifyParseResult {
    let nq = normalize_name(&query);
    ClipboardModifyParseResult::Partial(PartialQuery {
        stage_index,
        section,
        query,
        suggestions: all
            .into_iter()
            .filter(|s| normalize_name(s).starts_with(&nq))
            .collect(),
    })
}
fn suggestions(q: &str) -> Vec<String> {
    operations()
        .iter()
        .flat_map(|o| {
            std::iter::once(o.command.to_string()).chain(o.aliases.iter().map(|s| s.to_string()))
        })
        .filter(|s| normalize_name(s).starts_with(q))
        .collect()
}

fn parse_stage(stage: &Stage, idx: usize) -> Result<StageSpec, ClipboardModifyParseResult> {
    if stage.tokens.is_empty() {
        return Err(ClipboardModifyParseResult::Invalid(err(
            Some(idx),
            stage.span.start,
            stage.span.end,
            ParserErrorKind::EmptyStage,
        )));
    }
    if normalize_name(&stage.tokens[0].text) == "wrap" {
        return parse_wrap_stage(stage, idx);
    }
    let (op, consumed) = longest_op(&stage.tokens).ok_or_else(|| {
        ClipboardModifyParseResult::Invalid(err(
            Some(idx),
            stage.tokens[0].span.start,
            stage.tokens[0].span.end,
            ParserErrorKind::UnknownCommand(stage.tokens[0].text.clone()),
        ))
    })?;
    let args = &stage.tokens[consumed..];
    let mut a = StageArguments::default();
    match op.argument_requirements {
        ArgumentRequirements::None => {
            if let Some(t) = args.first() {
                return Err(ClipboardModifyParseResult::Invalid(err(
                    Some(idx),
                    t.span.start,
                    t.span.end,
                    ParserErrorKind::UnexpectedArgument {
                        operation: op.command.into(),
                        argument: t.text.clone(),
                    },
                )));
            }
        }
        ArgumentRequirements::CodeBlock => {
            if args.len() > 1 {
                let t = &args[1];
                return Err(ClipboardModifyParseResult::Invalid(err(
                    Some(idx),
                    t.span.start,
                    t.span.end,
                    ParserErrorKind::UnexpectedArgument {
                        operation: op.command.into(),
                        argument: t.text.clone(),
                    },
                )));
            }
            a.language = args.first().map(|t| t.text.clone());
        }
        ArgumentRequirements::Template | ArgumentRequirements::NamedWrap => {
            if args.is_empty() {
                return Err(ClipboardModifyParseResult::Invalid(err(
                    Some(idx),
                    stage.span.end,
                    stage.span.end,
                    ParserErrorKind::MissingArgument {
                        operation: op.command.into(),
                        argument: "name",
                    },
                )));
            };
            a.name = Some(join_tokens(args));
        }
        ArgumentRequirements::CustomWrap => {
            if args.len() < 2 {
                return Err(ClipboardModifyParseResult::Invalid(err(
                    Some(idx),
                    stage.span.end,
                    stage.span.end,
                    ParserErrorKind::MissingArgument {
                        operation: op.command.into(),
                        argument: if args.is_empty() { "prefix" } else { "suffix" },
                    },
                )));
            }
            if args.len() > 2 {
                let t = &args[2];
                return Err(ClipboardModifyParseResult::Invalid(err(
                    Some(idx),
                    t.span.start,
                    t.span.end,
                    ParserErrorKind::UnexpectedArgument {
                        operation: op.command.into(),
                        argument: t.text.clone(),
                    },
                )));
            }
            a.prefix = Some(args[0].text.clone());
            a.suffix = Some(args[1].text.clone());
        }
    }
    Ok(StageSpec {
        operation: op.id,
        arguments: a,
    })
}
fn parse_wrap_stage(stage: &Stage, idx: usize) -> Result<StageSpec, ClipboardModifyParseResult> {
    let args = &stage.tokens[1..];
    if args.is_empty() {
        return Err(ClipboardModifyParseResult::Invalid(err(
            Some(idx),
            stage.span.end,
            stage.span.end,
            ParserErrorKind::MissingArgument {
                operation: "wrap".into(),
                argument: "name",
            },
        )));
    }
    if args.len() == 1 {
        let name = args[0].text.clone();
        if is_known_wrapper(&name) {
            return Ok(StageSpec {
                operation: OperationId::NamedWrap,
                arguments: StageArguments {
                    name: Some(name),
                    ..Default::default()
                },
            });
        }
        return Err(ClipboardModifyParseResult::Invalid(err(
            Some(idx),
            args[0].span.start,
            args[0].span.end,
            ParserErrorKind::UnknownCommand(format!("wrap {}", args[0].text)),
        )));
    }
    if args.len() == 2 {
        return Ok(StageSpec {
            operation: OperationId::CustomWrap,
            arguments: StageArguments {
                prefix: Some(args[0].text.clone()),
                suffix: Some(args[1].text.clone()),
                ..Default::default()
            },
        });
    }
    let t = &args[2];
    Err(ClipboardModifyParseResult::Invalid(err(
        Some(idx),
        t.span.start,
        t.span.end,
        ParserErrorKind::UnexpectedArgument {
            operation: "wrap".into(),
            argument: t.text.clone(),
        },
    )))
}

fn is_known_wrapper(name: &str) -> bool {
    matches!(
        normalize_name(name).as_str(),
        "quotes" | "single-quote" | "double-quote" | "backticks" | "markdown-quote"
    )
}

fn longest_op(tokens: &[Token]) -> Option<(&'static super::catalog::OperationInfo, usize)> {
    let mut best = None;
    for end in 1..=tokens.len() {
        if tokens[end - 1].quoted {
            break;
        }
        let name = join_tokens(&tokens[..end]);
        if let Some(op) = operation_lookup(&name) {
            best = Some((op, end));
        }
    }
    best
}
fn join_tokens(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}
fn err(stage_index: Option<usize>, start: usize, end: usize, kind: ParserErrorKind) -> ParserError {
    ParserError {
        stage_index,
        span: Span { start, end },
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::default_catalog;
    fn p(s: &str) -> ClipboardModifyParseResult {
        parse(s, &default_catalog())
    }
    #[test]
    fn cm_case_and_open() {
        assert!(matches!(
            p("CM"),
            ClipboardModifyParseResult::OpenSection(ModifySection::Modify)
        ));
        assert!(matches!(
            p("xx"),
            ClipboardModifyParseResult::NotClipboardModify
        ));
    }
    #[test]
    fn aliases_and_canonicals() {
        for c in [
            "trim",
            "trim lines",
            "trim-lines",
            "trim_lines",
            "single quote",
            "code block rust",
            "sort descending",
            "json-pretty",
            "json-compact",
            "base64-encode",
            "url-decode",
            "lowercase",
            "title-case",
            "snake-case",
        ] {
            assert!(
                matches!(
                    p(&format!("cm {c}")),
                    ClipboardModifyParseResult::CompleteExecution(_)
                ),
                "{c}"
            );
        }
    }
    #[test]
    fn wrap_forms() {
        assert!(matches!(
            p("cm wrap quotes"),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
        assert!(matches!(
            p("cm wrap < >"),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
        assert!(matches!(
            p("cm wrap \"<!-- \" \" -->\""),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
        assert!(matches!(
            p("cm wrap only"),
            ClipboardModifyParseResult::Invalid(_)
        ));
    }
    #[test]
    fn quotes_escapes_pipes() {
        assert!(matches!(
            p("cm wrap \"\" \"\""),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
        assert!(matches!(
            p("cm wrap \"a b\" \"c d\""),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
        assert!(matches!(
            p("cm wrap \"a\\\"b\" c"),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
        assert!(matches!(
            p("cm wrap \"|\" x"),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
    }
    #[test]
    fn pipelines_and_bad_pipes() {
        assert!(matches!(
            p("cm trim | unique-lines | sort descending"),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
        for s in ["cm | trim", "cm trim |", "cm trim || unique"] {
            assert!(
                matches!(p(s), ClipboardModifyParseResult::Invalid(_)),
                "{s}"
            );
        }
    }
    #[test]
    fn invalid_args_and_quotes() {
        for s in [
            "cm trim extra",
            "cm custom-wrap <",
            "cm code rust extra",
            "cm wrap \"unterminated",
            "cm wrap \\",
        ] {
            assert!(
                matches!(p(s), ClipboardModifyParseResult::Invalid(_)),
                "{s}"
            );
        }
    }
    #[test]
    fn templates_apply_partials_unknowns() {
        assert!(matches!(
            p("cm template"),
            ClipboardModifyParseResult::OpenSection(ModifySection::Templates)
        ));
        assert!(matches!(
            p("cm apply"),
            ClipboardModifyParseResult::OpenSection(ModifySection::SavedPipelines)
        ));
        assert!(matches!(
            p("cm template p"),
            ClipboardModifyParseResult::Partial(_)
        ));
        assert!(matches!(
            p("cm apply c"),
            ClipboardModifyParseResult::Partial(_)
        ));
        assert!(matches!(
            p("cm template prompt context"),
            ClipboardModifyParseResult::CompleteExecution(_)
        ));
        assert!(matches!(
            p("cm template no-such"),
            ClipboardModifyParseResult::Partial(_)
        ));
        assert!(matches!(
            p("cm apply no-such"),
            ClipboardModifyParseResult::Partial(_)
        ));
    }

    #[test]
    fn navigation_commands_open_sections_case_insensitively() {
        for (query, section) in [
            ("cm modify", ModifySection::Modify),
            ("CM MODIFY", ModifySection::Modify),
            ("cm template", ModifySection::Templates),
            ("CM TEMPLATE", ModifySection::Templates),
            ("cm apply", ModifySection::SavedPipelines),
            ("CM APPLY", ModifySection::SavedPipelines),
            ("cm manage-templates", ModifySection::ManageTemplates),
            ("CM MANAGE_TEMPLATES", ModifySection::ManageTemplates),
            ("cm manage-pipelines", ModifySection::ManagePipelines),
            ("CM MANAGE-PIPELINES", ModifySection::ManagePipelines),
            ("cm help", ModifySection::Help),
            ("CM HELP", ModifySection::Help),
        ] {
            assert_eq!(
                p(query),
                ClipboardModifyParseResult::OpenSection(section),
                "{query}"
            );
        }
    }

    #[test]
    fn template_apply_execution_and_undo_keep_existing_meanings() {
        assert_eq!(
            p("cm template prompt context"),
            ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::ApplyTemplate {
                name: "prompt context".into()
            })
        );
        assert_eq!(
            p("CM APPLY clean lines"),
            ClipboardModifyParseResult::CompleteExecution(
                ClipboardModifyIntent::ApplySavedPipeline {
                    name: "clean lines".into()
                }
            )
        );
        assert_eq!(
            p("Cm UnDo"),
            ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::Undo)
        );
    }
    #[test]
    fn incomplete_categories() {
        for s in ["cm code", "cm json", "cm wrap"] {
            assert!(
                matches!(
                    p(s),
                    ClipboardModifyParseResult::Partial(_)
                        | ClipboardModifyParseResult::OpenSection(_)
                ),
                "{s}"
            );
        }
    }
}
