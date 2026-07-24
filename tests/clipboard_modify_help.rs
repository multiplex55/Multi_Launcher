use multi_launcher::clipboard_modify::catalog::{control_commands, operations, wrappers};
use multi_launcher::clipboard_modify::help::build_help_entries;
use multi_launcher::clipboard_modify::model::*;
use multi_launcher::clipboard_modify::parser::{
    ClipboardModifyIntent, ClipboardModifyParseResult, parse,
};

fn catalog() -> ClipboardModifierCatalog {
    ClipboardModifierCatalog {
        templates: vec![ClipboardTemplate {
            id: "prompt-context".into(),
            label: "Prompt".into(),
            aliases: vec!["pc".into()],
            template: "<{{clipboard}}>".into(),
            processor: None,
        }],
        pipelines: vec![SavedPipeline {
            id: "clean-lines".into(),
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
fn help_contains_every_registered_operation_control_and_wrapper_alias() {
    let entries = build_help_entries(&catalog());

    for op in operations() {
        let entry = entries
            .iter()
            .find(|entry| {
                entry
                    .canonical_syntax
                    .starts_with(&format!("cm {}", op.command))
            })
            .unwrap_or_else(|| panic!("missing operation help for {}", op.command));
        for alias in op.aliases {
            assert!(
                entry.aliases.iter().any(|a| a == alias),
                "missing alias {alias}"
            );
        }
        for example in op.help_examples {
            assert!(entry.examples.iter().any(|e| e == &format!("cm {example}")));
        }
    }

    for control in control_commands() {
        let entry = entries
            .iter()
            .find(|entry| entry.canonical_syntax == control.syntax)
            .unwrap_or_else(|| panic!("missing control help for {}", control.syntax));
        for alias in control.aliases {
            assert!(
                entry.aliases.iter().any(|a| a == alias),
                "missing control alias {alias}"
            );
        }
    }

    for wrapper in wrappers() {
        let entry = entries
            .iter()
            .find(|entry| {
                entry
                    .canonical_syntax
                    .starts_with(&format!("cm {}", wrapper.command))
            })
            .unwrap_or_else(|| panic!("missing wrapper help for {}", wrapper.command));
        for alias in wrapper.aliases {
            assert!(
                entry.aliases.iter().any(|a| a == alias),
                "missing wrapper alias {alias}"
            );
        }
    }
}

#[test]
fn dynamic_catalog_entries_and_aliases_are_in_help() {
    let entries = build_help_entries(&catalog());
    let template = entries
        .iter()
        .find(|entry| entry.canonical_syntax == "cm template prompt-context")
        .unwrap();
    assert!(template.aliases.contains(&"pc".to_string()));
    assert!(template.examples.contains(&"cm template pc".to_string()));

    let pipeline = entries
        .iter()
        .find(|entry| entry.canonical_syntax == "cm apply clean-lines")
        .unwrap();
    assert!(pipeline.aliases.contains(&"cl".to_string()));
    assert!(pipeline.examples.contains(&"cm apply cl".to_string()));
}

#[test]
fn operation_pipeline_allowed_help_flag_matches_parser_behavior() {
    let catalog = catalog();
    let entries = build_help_entries(&catalog);
    for op in operations() {
        let query = match op.argument_requirements {
            multi_launcher::clipboard_modify::catalog::ArgumentRequirements::None => {
                format!("cm trim | {}", op.command)
            }
            multi_launcher::clipboard_modify::catalog::ArgumentRequirements::CustomWrap => {
                format!("cm trim | {} '<' '>'", op.command)
            }
            multi_launcher::clipboard_modify::catalog::ArgumentRequirements::NamedWrap
            | multi_launcher::clipboard_modify::catalog::ArgumentRequirements::Template => {
                format!("cm trim | {} pc", op.command)
            }
            multi_launcher::clipboard_modify::catalog::ArgumentRequirements::CodeBlock => {
                format!("cm trim | {} rust", op.command)
            }
        };
        let parses = matches!(
            parse(&query, &catalog),
            ClipboardModifyParseResult::CompleteExecution(ClipboardModifyIntent::Stages(_))
        );
        let entry = entries
            .iter()
            .find(|entry| {
                entry
                    .canonical_syntax
                    .starts_with(&format!("cm {}", op.command))
            })
            .unwrap();
        assert_eq!(entry.pipeline_allowed, parses, "{}", op.command);
    }
}
