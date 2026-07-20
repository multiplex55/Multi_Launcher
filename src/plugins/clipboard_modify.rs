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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardModifyPluginSettings {
    pub dialog_width: f32,
    pub dialog_height: f32,
    pub navigation_width: f32,
    pub source_preview_split_ratio: f32,
    pub template_filter: String,
    pub pipeline_filter: String,
    pub management_sort_field: String,
    pub management_sort_ascending: bool,
}

impl Default for ClipboardModifyPluginSettings {
    fn default() -> Self {
        Self {
            dialog_width: 900.0,
            dialog_height: 640.0,
            navigation_width: 150.0,
            source_preview_split_ratio: 0.5,
            template_filter: String::new(),
            pipeline_filter: String::new(),
            management_sort_field: "name".into(),
            management_sort_ascending: true,
        }
    }
}

pub fn migrate_enablement(settings: &mut crate::settings::Settings) -> bool {
    if settings.plugin_settings.contains_key("clipboard_modify") {
        return false;
    }
    settings.plugin_settings.insert(
        "clipboard_modify".into(),
        serde_json::to_value(ClipboardModifyPluginSettings::default())
            .expect("clipboard modify settings serialize"),
    );
    if let Some(enabled) = settings.enabled_plugins.as_mut() {
        enabled.insert("clipboard_modify".into());
    }
    true
}

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
                desc: format!(
                    "Clipboard Modify syntax is invalid: {}. Open `cm` and use the Help section for syntax examples.",
                    parser_error_hint(&error.kind)
                ),
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

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(ClipboardModifyPluginSettings::default()).ok()
    }

    fn settings_ui(&mut self, ui: &mut eframe::egui::Ui, value: &mut serde_json::Value) {
        let mut cfg = serde_json::from_value::<ClipboardModifyPluginSettings>(value.clone())
            .unwrap_or_default();
        ui.small("Clipboard Modify stores only UI preferences here; templates, pipelines, source text, previews, and undo text are not stored in settings.");
        ui.horizontal(|ui| {
            ui.label("Dialog size");
            ui.add(eframe::egui::DragValue::new(&mut cfg.dialog_width).clamp_range(320.0..=2400.0));
            ui.add(
                eframe::egui::DragValue::new(&mut cfg.dialog_height).clamp_range(240.0..=1600.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Navigation width");
            ui.add(
                eframe::egui::DragValue::new(&mut cfg.navigation_width).clamp_range(80.0..=500.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Source/preview split");
            ui.add(eframe::egui::Slider::new(
                &mut cfg.source_preview_split_ratio,
                0.1..=0.9,
            ));
        });
        ui.horizontal(|ui| {
            ui.label("Template filter");
            ui.text_edit_singleline(&mut cfg.template_filter);
        });
        ui.horizontal(|ui| {
            ui.label("Pipeline filter");
            ui.text_edit_singleline(&mut cfg.pipeline_filter);
        });
        ui.horizontal(|ui| {
            ui.label("Management sort");
            ui.text_edit_singleline(&mut cfg.management_sort_field);
        });
        ui.checkbox(&mut cfg.management_sort_ascending, "Sort ascending");
        *value = serde_json::to_value(cfg).unwrap_or(serde_json::Value::Null);
    }
}

fn parser_error_hint(kind: &crate::clipboard_modify::parser::ParserErrorKind) -> String {
    match kind {
        crate::clipboard_modify::parser::ParserErrorKind::UnterminatedQuote => {
            "unterminated quote".into()
        }
        crate::clipboard_modify::parser::ParserErrorKind::TrailingEscape => {
            "trailing escape".into()
        }
        crate::clipboard_modify::parser::ParserErrorKind::LeadingPipe => {
            "pipeline cannot start with `|`".into()
        }
        crate::clipboard_modify::parser::ParserErrorKind::TrailingPipe => {
            "pipeline cannot end with `|`".into()
        }
        crate::clipboard_modify::parser::ParserErrorKind::EmptyStage => {
            "pipeline contains an empty stage".into()
        }
        crate::clipboard_modify::parser::ParserErrorKind::UnknownCommand(c) => {
            format!("unknown command `{c}`")
        }
        crate::clipboard_modify::parser::ParserErrorKind::MissingArgument {
            operation,
            argument,
        } => format!("`{operation}` is missing `{argument}`"),
        crate::clipboard_modify::parser::ParserErrorKind::UnexpectedArgument {
            operation,
            argument,
        } => format!("`{operation}` does not accept `{argument}`"),
    }
}

fn section_action(section: ModifySection) -> Action {
    let section_slug = section_slug(section);
    let section_name = section_name(section);
    let payload = open_dialog_payload(section);
    let encoded = encode_action_payload(&payload).ok();
    Action {
        label: format!("Open Clipboard Modify {section_name}"),
        desc: format!("Open the Clipboard Modify {section_name} dialog section"),
        action: format!("clipboard_modify:open:{section_slug}"),
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
                desc: format!(
                    "Complete the query as `{}`",
                    suggestion_query(partial.section, &suggestion)
                ),
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
                desc: format!(
                    "Runs {stage_count} Clipboard Modify stage(s); reads the current clipboard and writes the transformed result"
                ),
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
                desc: format!(
                    "Applies template `{name}` immediately; reads the current clipboard and writes the transformed result"
                ),
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
                desc: format!(
                    "Runs saved pipeline `{name}` immediately; reads the current clipboard and writes the transformed result"
                ),
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
                desc: "Restores the clipboard text captured before the last Clipboard Modify write"
                    .into(),
                action: "clipboard_modify:undo".into(),
                args: encoded,
            }
        }
    }
}

fn section_name(section: ModifySection) -> &'static str {
    match section {
        ModifySection::Modify => "Modify",
        ModifySection::Templates => "Templates",
        ModifySection::SavedPipelines => "Saved Pipelines",
    }
}

fn section_slug(section: ModifySection) -> &'static str {
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
    use std::collections::HashSet;

    fn plugin() -> ClipboardModifyPlugin {
        ClipboardModifyPlugin::new(shared_default_catalog())
    }

    #[test]
    fn enablement_migration_keeps_default_enabled_plugins_none() {
        let mut settings = crate::settings::Settings::default();
        assert!(migrate_enablement(&mut settings));
        assert!(settings.plugin_settings.contains_key("clipboard_modify"));
        assert!(settings.enabled_plugins.is_none());
    }

    #[test]
    fn enablement_migration_adds_to_explicit_old_set_once() {
        let mut settings = crate::settings::Settings::default();
        settings.enabled_plugins = Some(HashSet::from(["calculator".to_owned()]));
        assert!(migrate_enablement(&mut settings));
        assert!(
            settings
                .enabled_plugins
                .as_ref()
                .unwrap()
                .contains("clipboard_modify")
        );
        let len = settings.enabled_plugins.as_ref().unwrap().len();
        assert!(!migrate_enablement(&mut settings));
        assert_eq!(settings.enabled_plugins.as_ref().unwrap().len(), len);
    }

    #[test]
    fn user_disabled_clipboard_modify_remains_disabled_after_settings_exist() {
        let mut settings = crate::settings::Settings::default();
        settings.enabled_plugins = Some(HashSet::from(["calculator".to_owned()]));
        settings.plugin_settings.insert(
            "clipboard_modify".into(),
            serde_json::to_value(ClipboardModifyPluginSettings::default()).unwrap(),
        );
        assert!(!migrate_enablement(&mut settings));
        assert!(
            !settings
                .enabled_plugins
                .as_ref()
                .unwrap()
                .contains("clipboard_modify")
        );
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
    fn contextual_descriptions_explain_actions() {
        let p = plugin();
        assert!(p.search("cm")[0].desc.contains("Modify dialog section"));
        assert!(
            p.search("cm template")[0]
                .desc
                .contains("Templates dialog section")
        );
        assert!(
            p.search("cm apply")[0]
                .desc
                .contains("Saved Pipelines dialog section")
        );
        assert!(
            p.search("cm up")[0]
                .desc
                .contains("Complete the query as `cm upper")
        );
        assert!(
            p.search("cm upper")[0]
                .desc
                .contains("reads the current clipboard and writes the transformed result")
        );
        assert!(p.search("cm |")[0].desc.contains("Help section"));
    }

    #[test]
    fn template_and_saved_pipeline_descriptions_identify_immediate_execution() {
        let p = plugin();
        assert!(
            p.search("cm template prompt context")[0]
                .desc
                .contains("template `prompt context` immediately")
        );
        assert!(
            p.search("cm apply clean lines")[0]
                .desc
                .contains("saved pipeline `clean lines` immediately")
        );
    }

    #[test]
    fn search_does_not_touch_clipboard_services() {
        let _ = plugin().search("cm upper");
    }
}
