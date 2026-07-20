use multi_launcher::clipboard_modify::catalog::operations;
use multi_launcher::clipboard_modify::model::*;
use multi_launcher::clipboard_modify::parser::*;

fn catalog() -> ClipboardModifierCatalog {
    ClipboardModifierCatalog {
        templates: vec![ClipboardTemplate {
            id: "prompt context".into(),
            label: "Prompt".into(),
            aliases: vec!["pc".into()],
            template: "<{{clipboard}}>".into(),
            processor: None,
        }],
        pipelines: vec![SavedPipeline {
            id: "clean lines".into(),
            label: "Clean".into(),
            aliases: vec!["cl".into()],
            stages: vec![StageSpec {
                operation: OperationId::TrimLines,
                arguments: StageArguments::default(),
            }],
        }],
    }
}

#[test]
fn every_baseline_command_parses_to_execution() {
    let cat = catalog();
    for op in operations() {
        let query = match op.argument_requirements {
            multi_launcher::clipboard_modify::catalog::ArgumentRequirements::None => {
                format!("cm {}", op.command)
            }
            multi_launcher::clipboard_modify::catalog::ArgumentRequirements::CustomWrap => {
                format!("cm {} '<' '>'", op.command)
            }
            multi_launcher::clipboard_modify::catalog::ArgumentRequirements::NamedWrap
            | multi_launcher::clipboard_modify::catalog::ArgumentRequirements::Template => {
                format!("cm {} 'pc'", op.command)
            }
            multi_launcher::clipboard_modify::catalog::ArgumentRequirements::CodeBlock => {
                format!("cm {} rust", op.command)
            }
        };
        assert!(
            matches!(
                parse(&query, &cat),
                ClipboardModifyParseResult::CompleteExecution(_)
            ),
            "{query}"
        );
    }
}

#[test]
fn aliases_and_sections_parse() {
    let cat = catalog();
    let cases = [
        (
            "cm",
            ClipboardModifyParseResult::OpenSection(ModifySection::Modify),
        ),
        (
            "cm template",
            ClipboardModifyParseResult::OpenSection(ModifySection::Templates),
        ),
        (
            "cm apply",
            ClipboardModifyParseResult::OpenSection(ModifySection::SavedPipelines),
        ),
    ];
    for (q, expected) in cases {
        assert_eq!(parse(q, &cat), expected);
    }
    assert!(
        matches!(parse("cm upper", &cat), ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::Stages(s)) if s[0].operation == OperationId::Uppercase)
    );
    assert!(matches!(
        parse("cm template pc", &cat),
        ClipboardModifyParseResult::Partial(_)
    ));
    assert!(
        matches!(parse("cm template 'prompt context'", &cat), ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::ApplyTemplate { name }) if name == "prompt context")
    );
    assert!(
        matches!(parse("cm apply 'clean lines'", &cat), ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::ApplySavedPipeline { name }) if name == "clean lines")
    );
}

#[test]
fn pipes_quoted_pipes_escapes_and_empty_arguments() {
    let cat = catalog();
    let r = parse("cm custom-wrap \"|\" \"\\\"\" | upper", &cat);
    match r {
        ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::Stages(s)) => {
            assert_eq!(s.len(), 2);
            assert_eq!(s[0].arguments.prefix.as_deref(), Some("|"));
            assert_eq!(s[0].arguments.suffix.as_deref(), Some("\""));
            assert_eq!(s[1].operation, OperationId::Uppercase);
        }
        other => panic!("unexpected {other:?}"),
    }

    let r = parse(r#"cm custom-wrap "" ''"#, &cat);
    match r {
        ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::Stages(s)) => {
            assert_eq!(s[0].arguments.prefix.as_deref(), Some(""));
            assert_eq!(s[0].arguments.suffix.as_deref(), Some(""));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn invalid_syntax_reports_spans_and_stage_indexes() {
    let cat = catalog();
    let cases = [
        ("cm | upper", None, ParserErrorKind::LeadingPipe),
        ("cm upper |", Some(1), ParserErrorKind::TrailingPipe),
        ("cm upper || lower", Some(1), ParserErrorKind::EmptyStage),
        (
            "cm nope",
            Some(0),
            ParserErrorKind::UnknownCommand("nope".into()),
        ),
        (
            "cm custom-wrap <",
            Some(0),
            ParserErrorKind::MissingArgument {
                operation: "custom-wrap".into(),
                argument: "suffix",
            },
        ),
        (
            "cm upper extra",
            Some(0),
            ParserErrorKind::UnexpectedArgument {
                operation: "uppercase".into(),
                argument: "extra".into(),
            },
        ),
        ("cm 'unterminated", None, ParserErrorKind::UnterminatedQuote),
        ("cm upper \\", None, ParserErrorKind::TrailingEscape),
    ];
    for (q, stage, kind) in cases {
        match parse(q, &cat) {
            ClipboardModifyParseResult::Invalid(e) => {
                assert_eq!(e.stage_index, stage, "{q}");
                assert_eq!(e.kind, kind, "{q}");
                assert!(e.span.end >= e.span.start, "{q}");
            }
            other => panic!("{q} -> {other:?}"),
        }
    }
}
