use multi_launcher::clipboard_modify::actions::{
    ClipboardModifyActionPayload, decode_action_payload,
};
use multi_launcher::clipboard_modify::model::*;
use multi_launcher::clipboard_modify::store::shared_default_catalog;
use multi_launcher::plugin::{Plugin, PluginManager};
use multi_launcher::plugins::clipboard_modify::ClipboardModifyPlugin;
use std::collections::HashSet;
use std::sync::Arc;

fn custom_catalog() -> ClipboardModifierCatalog {
    ClipboardModifierCatalog {
        templates: vec![ClipboardTemplate {
            id: "dynamic template".into(),
            label: "Dynamic".into(),
            aliases: vec!["dt".into()],
            template: "{{clipboard}}!".into(),
            processor: None,
        }],
        pipelines: vec![SavedPipeline {
            id: "dynamic pipeline".into(),
            label: "Dynamic Pipe".into(),
            aliases: vec!["dp".into()],
            stages: vec![StageSpec {
                operation: OperationId::Uppercase,
                arguments: StageArguments::default(),
            }],
        }],
    }
}

#[test]
fn prefix_routing_and_actions() {
    let p = ClipboardModifyPlugin::new(shared_default_catalog());
    assert_eq!(p.query_prefixes(), &["cm"]);
    assert!(p.search("clipboard modify").is_empty());
    assert!(p.search("case upper").is_empty());
    assert_eq!(p.search("cm")[0].action, "clipboard_modify:open:modify");
    assert_eq!(
        p.search("cm template")[0].action,
        "clipboard_modify:open:templates"
    );
    assert_eq!(
        p.search("cm apply")[0].action,
        "clipboard_modify:open:saved-pipelines"
    );
    assert!(p.search("cm up")[0].action.starts_with("query:cm upper"));
    let complete = p.search("cm upper").remove(0);
    assert_eq!(complete.action, "clipboard_modify:execute");
    let payload: ClipboardModifyActionPayload =
        decode_action_payload(&complete.args.unwrap()).expect("typed execute payload");
    assert!(matches!(
        payload,
        ClipboardModifyActionPayload::ExecuteAdHocStages { ref stages, .. }
            if stages.iter().any(|stage| stage.operation == OperationId::Uppercase)
    ));
}

#[test]
fn dynamic_templates_and_pipelines_are_suggested() {
    let shared = shared_default_catalog();
    *shared.write().unwrap() = Arc::new(custom_catalog());
    let p = ClipboardModifyPlugin::new(shared);
    assert!(
        p.search("cm template dyn")
            .iter()
            .any(|a| a.action == "query:cm template dynamic template")
    );
    assert!(
        p.search("cm apply dyn")
            .iter()
            .any(|a| a.action == "query:cm apply dynamic pipeline")
    );
}

#[test]
fn plugin_manager_enablement_only_disables_clipboard_modify_itself() {
    let mut pm = PluginManager::new();
    pm.register(Box::new(ClipboardModifyPlugin::new(
        shared_default_catalog(),
    )));
    assert!(!pm.search_filtered("cm upper", None, None).is_empty());

    let none = HashSet::new();
    assert!(pm.search_filtered("cm upper", Some(&none), None).is_empty());

    for enabled in [
        HashSet::from(["clipboard_modify".to_string(), "clipboard".to_string()]),
        HashSet::from(["clipboard_modify".to_string(), "text_case".to_string()]),
    ] {
        assert!(
            !pm.search_filtered("cm upper", Some(&enabled), None)
                .is_empty()
        );
        assert!(
            !pm.commands_filtered(Some(&enabled))
                .iter()
                .any(|a| a.label == "cb")
        );
    }
}
