use crate::actions::Action;
use crate::clipboard_modify::actions::{
    OPEN_PREFIX, encode_action_payload, execute_saved_pipeline_payload, execute_stages_payload,
    execute_template_payload, open_dialog_payload, undo_payload,
};
use crate::clipboard_modify::catalog::{canonical_command, normalize_name, operation_lookup};
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
        vec![
            command_action("cm", "Clipboard Modify"),
            command_action("cm template", "Open Clipboard Modify templates"),
            command_action("cm apply", "Open Clipboard Modify saved pipelines"),
            command_action("cm undo", "Undo last Clipboard Modify"),
            command_action("cm upper", "Clipboard Modify uppercase text"),
            command_action("cm trim", "Clipboard Modify trim text"),
            command_action("cm wrap", "Clipboard Modify wrap text"),
            command_action("cm sort", "Clipboard Modify sort lines"),
        ]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["cm"]
    }
}

fn section_action(section: ModifySection) -> Action {
    let section_name = section_name(section);
    let payload = open_dialog_payload(section);
    let encoded = encode_action_payload(&payload).ok();
    Action {
        label: format!("Open Clipboard Modify {section_name}"),
        desc: "Clipboard Modify".into(),
        action: format!("clipboard_modify:open:{section_name}"),
        args: encoded,
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

fn command_action(query: &str, desc: &str) -> Action {
    Action {
        label: query.into(),
        desc: desc.into(),
        action: format!("query:{query}"),
        args: None,
    }
}

fn execution_action(intent: ClipboardModifyIntent) -> Action {
    match intent {
        ClipboardModifyIntent::Stages(stages) => {
            let stage_count = stages.len();
            let payload = execute_stages_payload(stages);
            Action {
                label: "Run Clipboard Modify pipeline".into(),
                desc: format!("Clipboard Modify: {stage_count} stage(s)"),
                action: "clipboard_modify:execute".into(),
                args: serde_json::to_string(&serde_json::json!({
                    "intent": "stages",
                    "stages": payload,
                }))
                .ok(),
            }
        }
        ClipboardModifyIntent::ApplyTemplate { name } => {
            let normalized = normalize_name(&name);
            let payload = execute_template_payload(normalized);
            Action {
                label: format!("Apply Clipboard Modify template {name}"),
                desc: "Clipboard Modify".into(),
                action: "clipboard_modify:execute".into(),
                args: serde_json::to_string(&serde_json::json!({
                    "intent": "template",
                    "payload": payload,
                }))
                .ok(),
            }
        }
        ClipboardModifyIntent::ApplySavedPipeline { name } => {
            let normalized = normalize_name(&name);
            let payload = execute_saved_pipeline_payload(normalized);
            Action {
                label: format!("Run Clipboard Modify pipeline {name}"),
                desc: "Clipboard Modify".into(),
                action: "clipboard_modify:execute".into(),
                args: serde_json::to_string(&serde_json::json!({
                    "intent": "saved-pipeline",
                    "payload": payload,
                }))
                .ok(),
            }
        }
        ClipboardModifyIntent::Undo => {
            let payload = undo_payload();
            let encoded = encode_action_payload(&payload).ok();
            Action {
                label: "Undo Clipboard Modify".into(),
                desc: "Clipboard Modify".into(),
                action: "clipboard_modify:undo".into(),
                args: encoded,
            }
        }
    }
}

fn section_name(section: ModifySection) -> &'static str {
    match section {
        ModifySection::Modify => "modify",
        ModifySection::Templates => "templates",
        ModifySection::SavedPipelines => "saved-pipelines",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::actions::{
        ClipboardModifyActionPayload, ClipboardModifySectionPayload, decode_action_payload,
    };
    use crate::clipboard_modify::store::shared_default_catalog;

    fn plugin() -> ClipboardModifyPlugin {
        ClipboardModifyPlugin::new(shared_default_catalog())
    }

    fn payload(action: &Action) -> ClipboardModifyActionPayload {
        let args = action.args.as_deref().expect("payload args");
        decode_action_payload(args).unwrap()
    }

    #[test]
    fn bare_category_commands_open_targeted_sections() {
        assert_eq!(
            payload(&plugin().search("cm").remove(0)),
            ClipboardModifyActionPayload::OpenDialogSection {
                section: ClipboardModifySectionPayload::Modify
            }
        );
        assert_eq!(
            payload(&plugin().search("cm template").remove(0)),
            ClipboardModifyActionPayload::OpenDialogSection {
                section: ClipboardModifySectionPayload::Templates
            }
        );
        assert_eq!(
            payload(&plugin().search("cm apply").remove(0)),
            ClipboardModifyActionPayload::OpenDialogSection {
                section: ClipboardModifySectionPayload::SavedPipelines
            }
        );
    }

    #[test]
    fn undo_emits_typed_payload() {
        assert_eq!(
            payload(&plugin().search("cm undo").remove(0)),
            ClipboardModifyActionPayload::Undo
        );
    }

    #[test]
    fn syntax_errors_are_non_executing_error_actions() {
        let result = plugin().search("cm |");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].action, "clipboard_modify:error");
        assert!(result[0].args.is_none());
    }

    #[test]
    fn search_does_not_touch_clipboard_services() {
        let _ = plugin().search("cm upper");
    }
}
