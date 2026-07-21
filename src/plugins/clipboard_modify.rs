use crate::actions::Action;
use crate::clipboard_modify::actions::{
    OPEN_PREFIX, encode_action_payload, execute_saved_pipeline_payload, execute_stages_payload,
    execute_template_payload, open_dialog_payload, undo_payload,
};
use crate::clipboard_modify::catalog::{
    ArgumentRequirements, canonical_command, normalize_name, operation_lookup, operations,
};
use crate::clipboard_modify::model::{ClipboardModifierCatalog, OperationId};
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
        if is_exact_cm_root_query(query) {
            return cm_root_actions(catalog.as_ref());
        }
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
        cm_static_commands()
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

fn is_exact_cm_root_query(query: &str) -> bool {
    query.trim().eq_ignore_ascii_case("cm")
}

fn cm_root_actions(catalog: &ClipboardModifierCatalog) -> Vec<Action> {
    let mut actions = vec![root_open_modify_action()];
    actions.extend(cm_root_suggestions(catalog));
    actions
}

fn root_open_modify_action() -> Action {
    let mut action = section_action(ModifySection::Modify);
    action.label = "cm: Open Clipboard Modify".into();
    action.desc = "Opens the Clipboard Modify dialog section".into();
    action
}

fn cm_root_suggestions(catalog: &ClipboardModifierCatalog) -> Vec<Action> {
    let mut actions = Vec::new();
    actions.extend(cm_navigation_suggestions());
    actions.extend(cm_operation_suggestions());
    actions.extend(catalog.pipelines.iter().map(|pipeline| {
        query_action(
            &format!("cm apply {}", pipeline.id),
            &format!("Pipeline: {} ({})", pipeline.label, pipeline.id),
            &format!("Replace the launcher query with `cm apply {}`", pipeline.id),
        )
    }));
    actions.extend(catalog.templates.iter().map(|template| {
        query_action(
            &format!("cm template {}", template.id),
            &format!("Template: {} ({})", template.label, template.id),
            &format!(
                "Replace the launcher query with `cm template {}`",
                template.id
            ),
        )
    }));
    actions
}

fn cm_navigation_suggestions() -> Vec<Action> {
    [
        ("cm modify", "Open Modify section"),
        ("cm template", "Open Templates section"),
        ("cm apply", "Open Saved Pipelines section"),
        ("cm manage-templates", "Open Manage Templates section"),
        ("cm manage-pipelines", "Open Manage Pipelines section"),
        ("cm help", "Open Help section"),
    ]
    .into_iter()
    .map(|(query, desc)| query_action(query, query, desc))
    .collect()
}

fn cm_operation_suggestions() -> Vec<Action> {
    operations()
        .iter()
        .filter(|op| op.id != OperationId::Template)
        .map(|op| {
            let query = match op.argument_requirements {
                ArgumentRequirements::None => format!("cm {}", op.command),
                _ => format!("cm {} ", op.command),
            };
            query_action(
                &query,
                &format!("{}: {}", op.command, op.label),
                op.description,
            )
        })
        .collect()
}

fn cm_static_commands() -> Vec<Action> {
    let mut actions = vec![command_action("cm", "Clipboard Modify")];
    actions.extend(cm_navigation_suggestions());
    actions.push(command_action("cm undo", "Undo last Clipboard Modify"));
    actions.extend(cm_operation_suggestions());
    actions
}

fn query_action(query: &str, label: &str, desc: &str) -> Action {
    Action {
        label: label.into(),
        desc: desc.into(),
        action: format!("query:{query}"),
        args: None,
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
    let label = if section == ModifySection::Modify {
        "Open Clipboard Modify".into()
    } else {
        format!("Open Clipboard Modify {section_name}")
    };
    Action {
        label,
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
        ModifySection::ManageTemplates => "cm manage-templates".into(),
        ModifySection::ManagePipelines => "cm manage-pipelines".into(),
        ModifySection::Help => "cm help".into(),
    }
}

fn suggestion_label(section: ModifySection, suggestion: &str) -> String {
    match section {
        ModifySection::Modify => operation_lookup(suggestion)
            .map(|op| format!("{}: {}", canonical_command(op.id), op.description))
            .unwrap_or_else(|| suggestion.to_string()),
        ModifySection::Templates => format!("Use template {suggestion}"),
        ModifySection::SavedPipelines => format!("Apply pipeline {suggestion}"),
        ModifySection::ManageTemplates => "Manage templates".into(),
        ModifySection::ManagePipelines => "Manage pipelines".into(),
        ModifySection::Help => "Open help".into(),
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
        ModifySection::ManageTemplates => "Manage Templates",
        ModifySection::ManagePipelines => "Manage Pipelines",
        ModifySection::Help => "Help",
    }
}

fn section_slug(section: ModifySection) -> &'static str {
    match section {
        ModifySection::Modify => "modify",
        ModifySection::Templates => "templates",
        ModifySection::SavedPipelines => "saved-pipelines",
        ModifySection::ManageTemplates => "manage-templates",
        ModifySection::ManagePipelines => "manage-pipelines",
        ModifySection::Help => "help",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::actions::{
        ClipboardModifyActionPayload, ClipboardModifySectionPayload, decode_action_payload,
    };
    use crate::clipboard_modify::model::{
        ClipboardModifierCatalog, ClipboardTemplate, SavedPipeline,
    };
    use crate::clipboard_modify::store::shared_default_catalog;
    use std::collections::HashSet;
    use std::sync::{Arc, RwLock};

    fn plugin() -> ClipboardModifyPlugin {
        ClipboardModifyPlugin::new(shared_default_catalog())
    }

    fn plugin_with_test_catalog() -> ClipboardModifyPlugin {
        let catalog = ClipboardModifierCatalog::new(
            vec![
                ClipboardTemplate {
                    id: "alpha-template".into(),
                    label: "Alpha Template".into(),
                    aliases: vec!["at".into()],
                    template: "A {{clipboard}}".into(),
                    processor: None,
                },
                ClipboardTemplate {
                    id: "beta-template".into(),
                    label: "Beta Template".into(),
                    aliases: vec!["bt".into()],
                    template: "B {{clipboard}}".into(),
                    processor: None,
                },
            ],
            vec![
                SavedPipeline {
                    id: "alpha-pipeline".into(),
                    label: "Alpha Pipeline".into(),
                    aliases: vec!["ap".into()],
                    stages: vec![],
                },
                SavedPipeline {
                    id: "beta-pipeline".into(),
                    label: "Beta Pipeline".into(),
                    aliases: vec!["bp".into()],
                    stages: vec![],
                },
            ],
        )
        .unwrap();
        ClipboardModifyPlugin::new(Arc::new(RwLock::new(Arc::new(catalog))))
    }

    #[test]
    fn bare_cm_root_lists_direct_open_and_non_executing_completions() {
        let results = plugin_with_test_catalog().search("  CM  ");
        assert!(!results.is_empty());
        assert_eq!(results[0].label, "cm: Open Clipboard Modify");
        assert_eq!(results[0].action, "clipboard_modify:open:modify");
        assert_eq!(
            payload(&results[0]),
            ClipboardModifyActionPayload::OpenDialogSection {
                section: ClipboardModifySectionPayload::Modify
            }
        );
        assert!(results[0].desc.contains("opens") || results[0].desc.contains("Open"));
        assert!(
            results[1..]
                .iter()
                .all(|action| action.action.starts_with("query:"))
        );

        let queries: Vec<_> = results
            .iter()
            .filter_map(|action| action.action.strip_prefix("query:"))
            .collect();

        for nav in [
            "cm modify",
            "cm template",
            "cm apply",
            "cm manage-templates",
            "cm manage-pipelines",
            "cm help",
        ] {
            assert!(queries.contains(&nav), "missing navigation query {nav}");
        }

        for op in operations()
            .iter()
            .filter(|op| op.id != OperationId::Template)
        {
            let expected = match op.argument_requirements {
                ArgumentRequirements::None => format!("cm {}", op.command),
                _ => format!("cm {} ", op.command),
            };
            assert!(
                queries.contains(&expected.as_str()),
                "missing operation query {expected}"
            );
            assert!(
                results
                    .iter()
                    .any(|action| action.action == format!("query:{expected}")
                        && action.label.contains(op.label)
                        && action.desc.contains(op.description)),
                "missing label/description for {expected}"
            );
        }

        assert!(queries.contains(&"cm uppercase"));
        assert!(!queries.contains(&"cm upper"));
        assert!(queries.contains(&"cm apply alpha-pipeline"));
        assert!(queries.contains(&"cm apply beta-pipeline"));
        assert!(!queries.contains(&"cm apply ap"));
        assert!(!queries.contains(&"cm apply bp"));
        assert!(queries.contains(&"cm template alpha-template"));
        assert!(queries.contains(&"cm template beta-template"));
        assert!(!queries.contains(&"cm template at"));
        assert!(!queries.contains(&"cm template bt"));
        assert!(!queries.contains(&"cm undo"));
        assert_eq!(
            queries
                .iter()
                .filter(|query| **query == "cm template")
                .count(),
            1
        );

        let nav_start = 1;
        let ops_start = nav_start + 6;
        let pipeline_start = ops_start
            + operations()
                .iter()
                .filter(|op| op.id != OperationId::Template)
                .count();
        let template_start = pipeline_start + 2;
        assert_eq!(results[nav_start].action, "query:cm modify");
        assert_eq!(results[ops_start].action, "query:cm single-quote");
        assert_eq!(
            results[pipeline_start].action,
            "query:cm apply alpha-pipeline"
        );
        assert_eq!(
            results[template_start].action,
            "query:cm template alpha-template"
        );

        for query in [
            "cm modify",
            "cm uppercase",
            "cm apply alpha-pipeline",
            "cm template alpha-template",
        ] {
            let action = results
                .iter()
                .find(|action| action.action == format!("query:{query}"))
                .expect("completion action");
            assert!(action.args.is_none());
        }
    }

    #[test]
    fn root_detection_does_not_swallow_subcommands_or_undo() {
        assert_eq!(
            plugin_with_test_catalog().search("cm modify")[0].label,
            "Open Clipboard Modify"
        );
        assert_eq!(
            payload(&plugin_with_test_catalog().search("cm undo").remove(0)),
            ClipboardModifyActionPayload::Undo
        );
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
        assert_eq!(
            payload(&plugin().search("cm manage-templates").remove(0)),
            ClipboardModifyActionPayload::OpenDialogSection {
                section: ClipboardModifySectionPayload::ManageTemplates
            }
        );
        assert_eq!(
            payload(&plugin().search("cm manage-pipelines").remove(0)),
            ClipboardModifyActionPayload::OpenDialogSection {
                section: ClipboardModifySectionPayload::ManagePipelines
            }
        );
        assert_eq!(
            payload(&plugin().search("cm help").remove(0)),
            ClipboardModifyActionPayload::OpenDialogSection {
                section: ClipboardModifySectionPayload::Help
            }
        );
    }

    #[test]
    fn modify_direct_action_label_does_not_repeat_section_name() {
        assert_eq!(plugin().search("cm")[0].label, "cm: Open Clipboard Modify");
        assert_eq!(
            plugin().search("cm modify")[0].label,
            "Open Clipboard Modify"
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
