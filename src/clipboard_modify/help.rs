use super::catalog::{ArgumentRequirements, OperationCategory, OperationInfo, operations};
use super::model::ClipboardModifierCatalog;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpEntry {
    pub canonical_syntax: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub arguments: String,
    pub examples: Vec<String>,
    pub category: String,
    pub pipeline_allowed: bool,
}

impl HelpEntry {
    pub fn matches_filter(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        [
            self.canonical_syntax.as_str(),
            self.description.as_str(),
            self.arguments.as_str(),
            self.category.as_str(),
        ]
        .into_iter()
        .any(|s| s.to_lowercase().contains(&q))
            || self.aliases.iter().any(|s| s.to_lowercase().contains(&q))
            || self.examples.iter().any(|s| s.to_lowercase().contains(&q))
    }
}

pub fn build_help_entries(catalog: &ClipboardModifierCatalog) -> Vec<HelpEntry> {
    let mut entries = control_entries();
    entries.extend(operations().iter().map(operation_entry));
    entries.extend(catalog.templates.iter().map(|t| {
        HelpEntry {
            canonical_syntax: format!("cm template {}", t.id),
            description: format!("Apply template '{}' ({})", t.label, t.id),
            aliases: t.aliases.clone(),
            arguments: "template name or alias".into(),
            examples: std::iter::once(format!("cm template {}", t.id))
                .chain(t.aliases.iter().map(|a| format!("cm template {a}")))
                .collect(),
            category: "Template".into(),
            pipeline_allowed: false,
        }
    }));
    entries.extend(catalog.pipelines.iter().map(|p| {
        HelpEntry {
            canonical_syntax: format!("cm apply {}", p.id),
            description: format!("Run saved pipeline '{}' ({})", p.label, p.id),
            aliases: p.aliases.clone(),
            arguments: "saved pipeline name or alias".into(),
            examples: std::iter::once(format!("cm apply {}", p.id))
                .chain(p.aliases.iter().map(|a| format!("cm apply {a}")))
                .collect(),
            category: "Saved Pipeline".into(),
            pipeline_allowed: false,
        }
    }));
    entries
}

fn operation_entry(op: &OperationInfo) -> HelpEntry {
    HelpEntry {
        canonical_syntax: format!(
            "cm {}{}",
            op.command,
            argument_suffix(op.argument_requirements)
        ),
        description: op.description.into(),
        aliases: op.aliases.iter().map(|s| s.to_string()).collect(),
        arguments: argument_text(op.argument_requirements).into(),
        examples: op.help_examples.iter().map(|e| format!("cm {e}")).collect(),
        category: category_name(op.category).into(),
        pipeline_allowed: op.pipeline_available,
    }
}

fn control_entries() -> Vec<HelpEntry> {
    vec![
        control(
            "cm",
            "Open the Modify dialog section",
            "none",
            &["cm"],
            true,
        ),
        control(
            "cm template",
            "Open the Templates dialog section",
            "optional template name",
            &["cm template", "cm template prompt-context"],
            false,
        ),
        control(
            "cm apply",
            "Open the Saved Pipelines dialog section",
            "optional saved pipeline name",
            &["cm apply", "cm apply clean-lines"],
            false,
        ),
        control(
            "cm undo",
            "Restore the clipboard text captured before the last Clipboard Modify write",
            "none",
            &["cm undo"],
            false,
        ),
        control(
            "cm <stage> | <stage>",
            "Pipeline syntax: run stages left-to-right; syntax errors return help-oriented error results",
            "one or more pipeline-capable stages",
            &["cm trim-lines | unique-lines | sort-ascending"],
            false,
        ),
        control(
            "cm wrap <prefix> <suffix>",
            "Custom wrapper shorthand; quote prefixes/suffixes containing spaces, pipes, or quotes",
            "prefix and suffix",
            &["cm wrap \"<!-- \" \" -->\""],
            true,
        ),
    ]
}
fn control(
    syntax: &str,
    description: &str,
    arguments: &str,
    examples: &[&str],
    pipeline_allowed: bool,
) -> HelpEntry {
    HelpEntry {
        canonical_syntax: syntax.into(),
        description: description.into(),
        aliases: vec![],
        arguments: arguments.into(),
        examples: examples.iter().map(|s| s.to_string()).collect(),
        category: "Control".into(),
        pipeline_allowed,
    }
}
fn argument_suffix(req: ArgumentRequirements) -> &'static str {
    match req {
        ArgumentRequirements::None => "",
        ArgumentRequirements::CustomWrap => " <prefix> <suffix>",
        ArgumentRequirements::NamedWrap | ArgumentRequirements::Template => " <name>",
        ArgumentRequirements::CodeBlock => " [language]",
    }
}
fn argument_text(req: ArgumentRequirements) -> &'static str {
    match req {
        ArgumentRequirements::None => "none",
        ArgumentRequirements::CustomWrap => {
            "prefix and suffix; quote values with spaces or pipe characters"
        }
        ArgumentRequirements::NamedWrap => "named wrapper/template id or alias",
        ArgumentRequirements::Template => "template id or alias",
        ArgumentRequirements::CodeBlock => "optional language tag",
    }
}
fn category_name(c: OperationCategory) -> &'static str {
    match c {
        OperationCategory::Wrap => "Wrap",
        OperationCategory::Lines => "Lines",
        OperationCategory::Format => "Format",
        OperationCategory::Encoding => "Encoding",
        OperationCategory::Case => "Case",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::{default_catalog, operations};

    #[test]
    fn includes_every_registered_operation() {
        let entries = build_help_entries(&default_catalog());
        for op in operations() {
            assert!(
                entries.iter().any(|e| e
                    .canonical_syntax
                    .starts_with(&format!("cm {}", op.command))),
                "missing {}",
                op.command
            );
        }
    }

    #[test]
    fn aliases_and_examples_come_from_operation_metadata() {
        let entries = build_help_entries(&default_catalog());
        let sort = entries
            .iter()
            .find(|e| e.canonical_syntax == "cm sort-ascending")
            .unwrap();
        assert!(sort.aliases.contains(&"sort".into()));
        assert_eq!(sort.examples, vec!["cm sort-ascending"]);
    }

    #[test]
    fn templates_and_saved_pipelines_are_dynamic() {
        let mut catalog = default_catalog();
        catalog.templates[0].id = "dynamic-template".into();
        catalog.pipelines[0].id = "dynamic-pipeline".into();
        let entries = build_help_entries(&catalog);
        assert!(
            entries
                .iter()
                .any(|e| e.canonical_syntax == "cm template dynamic-template")
        );
        assert!(
            entries
                .iter()
                .any(|e| e.canonical_syntax == "cm apply dynamic-pipeline")
        );
    }
}
