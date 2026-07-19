use crate::actions::Action;
use crate::clipboard_modify::catalog::{canonical_command, operation_lookup};
use crate::clipboard_modify::parser::{
    ClipboardModifyIntent, ClipboardModifyParseResult, ModifySection, PartialQuery, parse,
};
use crate::clipboard_modify::store::SharedClipboardModifierCatalog;
use crate::plugin::Plugin;

pub struct ClipboardModifyPlugin {
    catalog: SharedClipboardModifierCatalog,
}

impl ClipboardModifyPlugin {
    pub fn new(catalog: SharedClipboardModifierCatalog) -> Self {
        Self { catalog }
    }

    fn catalog_snapshot(
        &self,
    ) -> std::sync::Arc<crate::clipboard_modify::model::ClipboardModifierCatalog> {
        self.catalog.read().unwrap().clone()
    }
}

impl Plugin for ClipboardModifyPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let catalog = self.catalog_snapshot();
        if query.trim() == "cm undo" {
            return vec![Action {
                label: "Undo Clipboard Modify".into(),
                desc: "Clipboard Modify".into(),
                action: "clipboard_modify:undo".into(),
                args: None,
            }];
        }
        match parse(query, catalog.as_ref()) {
            ClipboardModifyParseResult::NotClipboardModify => Vec::new(),
            ClipboardModifyParseResult::OpenSection(section) => vec![section_action(section)],
            ClipboardModifyParseResult::Partial(partial) => partial_actions(partial),
            ClipboardModifyParseResult::CompleteExecution(intent) => vec![execution_action(intent)],
            ClipboardModifyParseResult::Invalid(error) => vec![Action {
                label: "Invalid clipboard modify query".into(),
                desc: format!("Clipboard Modify: {:?}", error.kind),
                action: "clipboard_modify:error".into(),
                args: None,
            }],
        }
    }

    fn name(&self) -> &str {
        "clipboard_modify"
    }

    fn description(&self) -> &str {
        "Builds clipboard modification pipelines with prefix `cm`"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "cm".into(),
            desc: "Clipboard Modify".into(),
            action: "query:cm".into(),
            args: None,
        }]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["cm"]
    }
}

fn section_action(section: ModifySection) -> Action {
    let section_name = section_name(section);
    Action {
        label: format!("Open Clipboard Modify {section_name}"),
        desc: "Clipboard Modify".into(),
        action: format!("clipboard_modify:open:{section_name}"),
        args: serde_json::to_string(&serde_json::json!({ "section": section_name })).ok(),
    }
}

fn partial_actions(partial: PartialQuery) -> Vec<Action> {
    partial
        .suggestions
        .into_iter()
        .map(|suggestion| {
            let label = suggestion_label(partial.section, &suggestion);
            Action {
                label,
                desc: format!("Clipboard Modify stage {}", partial.stage_index + 1),
                action: format!("query:{}", suggestion_query(partial.section, &suggestion)),
                args: None,
            }
        })
        .collect()
}

fn suggestion_query(section: ModifySection, suggestion: &str) -> String {
    match section {
        ModifySection::Modify => format!("cm {suggestion}"),
        ModifySection::Templates => format!("cm template {suggestion}"),
        ModifySection::SavedPipelines => format!("cm apply {suggestion}"),
    }
}

fn suggestion_label(section: ModifySection, suggestion: &str) -> String {
    match section {
        ModifySection::Modify => operation_lookup(suggestion)
            .map(|op| format!("{}: {}", canonical_command(op.id), op.description))
            .unwrap_or_else(|| suggestion.to_string()),
        ModifySection::Templates => format!("Use template {suggestion}"),
        ModifySection::SavedPipelines => format!("Apply pipeline {suggestion}"),
    }
}

fn execution_action(intent: ClipboardModifyIntent) -> Action {
    match intent {
        ClipboardModifyIntent::Stages(stages) => Action {
            label: "Run Clipboard Modify pipeline".into(),
            desc: format!("Clipboard Modify: {} stage(s)", stages.len()),
            action: "clipboard_modify:execute".into(),
            args: serde_json::to_string(
                &serde_json::json!({ "intent": "stages", "stages": stages }),
            )
            .ok(),
        },
        ClipboardModifyIntent::ApplySavedPipeline { name } => Action {
            label: format!("Run Clipboard Modify pipeline {name}"),
            desc: "Clipboard Modify".into(),
            action: "clipboard_modify:execute".into(),
            args: serde_json::to_string(
                &serde_json::json!({ "intent": "saved-pipeline", "name": name }),
            )
            .ok(),
        },
    }
}

fn section_name(section: ModifySection) -> &'static str {
    match section {
        ModifySection::Modify => "modify",
        ModifySection::Templates => "templates",
        ModifySection::SavedPipelines => "saved-pipelines",
    }
}
