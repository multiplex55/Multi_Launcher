use super::model::*;

pub fn default_templates() -> Vec<ClipboardTemplate> {
    vec![
        ClipboardTemplate {
            id: "markdown-quote".into(),
            label: "Markdown quote".into(),
            aliases: vec!["md-quote".into()],
            template: "> {{clipboard}}".into(),
            processor: Some(TemplateProcessor::Literal),
        },
        ClipboardTemplate {
            id: "prompt-context".into(),
            label: "Prompt context".into(),
            aliases: vec!["context".into()],
            template: "Context:\n{{clipboard}}".into(),
            processor: Some(TemplateProcessor::Literal),
        },
        ClipboardTemplate {
            id: "xml-block".into(),
            label: "XML block".into(),
            aliases: vec!["xml".into()],
            template: "<clipboard>\n{{clipboard}}\n</clipboard>".into(),
            processor: Some(TemplateProcessor::Literal),
        },
        ClipboardTemplate {
            id: "rust-raw-string".into(),
            label: "Rust raw string".into(),
            aliases: vec!["raw-string".into()],
            template: "{{clipboard}}".into(),
            processor: Some(TemplateProcessor::RustRawString),
        },
        ClipboardTemplate {
            id: "sql-diagnostic".into(),
            label: "SQL diagnostic".into(),
            aliases: vec!["sql-diag".into()],
            template: "-- Diagnostic SQL\n{{clipboard}}".into(),
            processor: Some(TemplateProcessor::Literal),
        },
    ]
}

pub fn default_pipelines() -> Vec<SavedPipeline> {
    use OperationId::*;
    vec![
        pipeline(
            "clean-lines",
            "Clean lines",
            &["tidy-lines"],
            vec![stage(TrimLines), stage(UniqueLines)],
        ),
        pipeline(
            "sorted-unique-lines",
            "Sorted unique lines",
            &["sort-uniq"],
            vec![stage(TrimLines), stage(UniqueLines), stage(SortAscending)],
        ),
        pipeline(
            "rust-code",
            "Rust code",
            &["rs-code"],
            vec![StageSpec {
                operation: CodeBlock,
                arguments: StageArguments {
                    language: Some("rust".into()),
                    ..Default::default()
                },
            }],
        ),
        pipeline(
            "json-code",
            "JSON code",
            &["json-fence"],
            vec![
                stage(JsonPretty),
                StageSpec {
                    operation: CodeBlock,
                    arguments: StageArguments {
                        language: Some("json".into()),
                        ..Default::default()
                    },
                },
            ],
        ),
        pipeline(
            "apply-prompt-context",
            "Prompt context",
            &["prompt"],
            vec![StageSpec {
                operation: Template,
                arguments: StageArguments {
                    name: Some("prompt-context".into()),
                    ..Default::default()
                },
            }],
        ),
    ]
}

pub fn default_catalog() -> ClipboardModifierCatalog {
    ClipboardModifierCatalog {
        templates: default_templates(),
        pipelines: default_pipelines(),
    }
}

fn stage(operation: OperationId) -> StageSpec {
    StageSpec {
        operation,
        arguments: StageArguments::default(),
    }
}
fn pipeline(id: &str, label: &str, aliases: &[&str], stages: Vec<StageSpec>) -> SavedPipeline {
    SavedPipeline {
        id: id.into(),
        label: label.into(),
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
        stages,
    }
}
