use serde::{Deserialize, Serialize};

use super::catalog::{normalize_name, operation_by_id, reserved_names};
use super::error::ClipboardModifyError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationId {
    SingleQuote,
    DoubleQuote,
    Backticks,
    CustomWrap,
    NamedWrap,
    Template,
    CodeBlock,
    SortAscending,
    SortDescending,
    UniqueLines,
    Trim,
    TrimLines,
    JsonPretty,
    JsonMinify,
    Base64Encode,
    Base64Decode,
    UrlEncode,
    UrlDecode,
    Lowercase,
    Uppercase,
    TitleCase,
    CamelCase,
    PascalCase,
    SnakeCase,
    ScreamingSnake,
    KebabCase,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StageArguments {
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub name: Option<String>,
    pub language: Option<String>,
}

impl StageArguments {
    pub fn any_supplied(&self) -> bool {
        self.prefix.is_some()
            || self.suffix.is_some()
            || self.name.is_some()
            || self.language.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageSpec {
    pub operation: OperationId,
    #[serde(default)]
    pub arguments: StageArguments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TemplateProcessor {
    Literal,
    RustRawString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipboardTemplate {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub template: String,
    #[serde(default)]
    pub processor: Option<TemplateProcessor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedPipeline {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub stages: Vec<StageSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClipboardModifiersFile {
    pub templates: Vec<ClipboardTemplate>,
    pub pipelines: Vec<SavedPipeline>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardModifierCatalog {
    pub templates: Vec<ClipboardTemplate>,
    pub pipelines: Vec<SavedPipeline>,
}

impl StageSpec {
    pub fn validate(&self) -> Result<(), ClipboardModifyError> {
        let op = operation_by_id(self.operation).ok_or_else(|| {
            ClipboardModifyError::UnknownOperation {
                operation: format!("{:?}", self.operation),
            }
        })?;
        op.argument_requirements
            .validate(self.operation, &self.arguments)
    }
}

impl ClipboardTemplate {
    pub fn validate(&self) -> Result<(), ClipboardModifyError> {
        validate_label_and_aliases(&self.id, &self.label, &self.aliases)?;
        if !self.template.contains("{{clipboard}}") {
            return Err(ClipboardModifyError::MissingPlaceholder {
                template: self.id.clone(),
            });
        }
        Ok(())
    }

    pub fn render(&self, clipboard: &str) -> String {
        let processed = match self.processor.unwrap_or(TemplateProcessor::Literal) {
            TemplateProcessor::Literal => clipboard.to_string(),
            TemplateProcessor::RustRawString => rust_raw_string_literal(clipboard),
        };
        self.template.replace("{{clipboard}}", &processed)
    }
}

pub fn rust_raw_string_literal(text: &str) -> String {
    let mut hashes = 0usize;
    loop {
        let fence = format!("\"{}", "#".repeat(hashes));
        if !text.contains(&fence) {
            return format!("r{}\"{}\"{}", "#".repeat(hashes), text, "#".repeat(hashes));
        }
        hashes += 1;
    }
}

impl SavedPipeline {
    pub fn validate(&self) -> Result<(), ClipboardModifyError> {
        validate_label_and_aliases(&self.id, &self.label, &self.aliases)?;
        for stage in &self.stages {
            stage.validate()?;
        }
        Ok(())
    }
}

impl ClipboardModifierCatalog {
    pub fn new(
        templates: Vec<ClipboardTemplate>,
        pipelines: Vec<SavedPipeline>,
    ) -> Result<Self, ClipboardModifyError> {
        validate_namespace(&templates, &pipelines)?;
        for t in &templates {
            t.validate()?;
        }
        for p in &pipelines {
            p.validate()?;
        }
        Ok(Self {
            templates,
            pipelines,
        })
    }
}

fn validate_label_and_aliases(
    id: &str,
    label: &str,
    aliases: &[String],
) -> Result<(), ClipboardModifyError> {
    if label.trim().is_empty() {
        return Err(ClipboardModifyError::EmptyLabel {
            name: id.to_string(),
        });
    }
    for alias in aliases {
        if super::catalog::normalize_name(alias).is_empty()
            || alias.trim().contains(char::is_whitespace)
        {
            return Err(ClipboardModifyError::InvalidAlias {
                alias: alias.clone(),
            });
        }
    }
    Ok(())
}

pub fn validate_namespace(
    templates: &[ClipboardTemplate],
    pipelines: &[SavedPipeline],
) -> Result<(), ClipboardModifyError> {
    let reserved = reserved_names();
    let mut seen = std::collections::BTreeSet::new();
    for name in templates
        .iter()
        .flat_map(|t| std::iter::once(&t.id).chain(t.aliases.iter()))
        .chain(
            pipelines
                .iter()
                .flat_map(|p| std::iter::once(&p.id).chain(p.aliases.iter())),
        )
    {
        let n = normalize_name(name);
        if reserved.contains(&n) {
            return Err(ClipboardModifyError::ReservedName { name: n });
        }
        if !seen.insert(n.clone()) {
            return Err(ClipboardModifyError::DuplicateName { name: n });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::catalog::*;

    #[test]
    fn operation_id_serialization_names() {
        assert_eq!(
            serde_json::to_string(&OperationId::SingleQuote).unwrap(),
            "\"single-quote\""
        );
        assert_eq!(
            serde_json::to_string(&OperationId::CodeBlock).unwrap(),
            "\"code-block\""
        );
        assert_eq!(
            serde_json::to_string(&OperationId::SortDescending).unwrap(),
            "\"sort-descending\""
        );
        assert_eq!(
            serde_json::to_string(&OperationId::TrimLines).unwrap(),
            "\"trim-lines\""
        );
        assert_eq!(
            serde_json::to_string(&OperationId::JsonPretty).unwrap(),
            "\"json-pretty\""
        );
        assert_eq!(
            serde_json::to_string(&OperationId::Base64Decode).unwrap(),
            "\"base64-decode\""
        );
        assert_eq!(
            serde_json::to_string(&OperationId::ScreamingSnake).unwrap(),
            "\"screaming-snake\""
        );
    }

    #[test]
    fn registry_uniqueness() {
        let mut names = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        for op in operations() {
            assert!(ids.insert(format!("{:?}", op.id)));
            assert!(
                names.insert(normalize_name(op.command)),
                "duplicate {}",
                op.command
            );
            for alias in op.aliases {
                assert!(
                    names.insert(normalize_name(alias)),
                    "duplicate alias {alias}"
                );
            }
        }
    }

    #[test]
    fn alias_normalization_and_lookup() {
        assert_eq!(normalize_name("  Foo__ Bar---Baz  "), "foo-bar-baz");
        assert_eq!(
            operation_lookup("SORT DESC").unwrap().id,
            OperationId::SortDescending
        );
    }

    #[test]
    fn reserved_name_rejection() {
        let t = ClipboardTemplate {
            id: "template".into(),
            label: "Bad".into(),
            aliases: vec![],
            template: "{{clipboard}}".into(),
            processor: None,
        };
        assert!(matches!(
            ClipboardModifierCatalog::new(vec![t], vec![]),
            Err(ClipboardModifyError::ReservedName { .. })
        ));
    }

    #[test]
    fn template_placeholder_validation() {
        let t = ClipboardTemplate {
            id: "x".into(),
            label: "X".into(),
            aliases: vec![],
            template: "missing".into(),
            processor: None,
        };
        assert!(matches!(
            t.validate(),
            Err(ClipboardModifyError::MissingPlaceholder { .. })
        ));
    }

    #[test]
    fn pipeline_validation() {
        let good = StageSpec {
            operation: OperationId::CodeBlock,
            arguments: StageArguments {
                language: Some("rust".into()),
                ..Default::default()
            },
        };
        assert!(good.validate().is_ok());
        let bad = StageSpec {
            operation: OperationId::CustomWrap,
            arguments: StageArguments {
                prefix: Some("(".into()),
                ..Default::default()
            },
        };
        assert!(matches!(
            bad.validate(),
            Err(ClipboardModifyError::MissingArgument {
                argument: "suffix",
                ..
            })
        ));
        let no_args = StageSpec {
            operation: OperationId::TrimLines,
            arguments: StageArguments {
                name: Some("x".into()),
                ..Default::default()
            },
        };
        assert!(matches!(
            no_args.validate(),
            Err(ClipboardModifyError::UnexpectedArgument { .. })
        ));
    }

    #[test]
    fn unknown_field_rejection_through_serde() {
        let json = r#"{"operation":"trim-lines","arguments":{"extra":"nope"}}"#;
        assert!(serde_json::from_str::<StageSpec>(json).is_err());
    }

    #[test]
    fn raw_string_processor_collision_safety() {
        let text = "contains \"# and \"## fences";
        let literal = rust_raw_string_literal(text);
        assert!(literal.starts_with("r###\""));
        assert!(literal.ends_with("\"###"));
        let t = ClipboardTemplate {
            id: "raw".into(),
            label: "Raw".into(),
            aliases: vec![],
            template: "{{clipboard}}".into(),
            processor: Some(TemplateProcessor::RustRawString),
        };
        assert_eq!(t.render(text), literal);
    }
}
