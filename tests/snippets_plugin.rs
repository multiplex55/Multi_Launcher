use multi_launcher::commands::{Command, StorageCommand, parse_action};
use multi_launcher::launcher::launch_action;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::snippets::{
    SNIPPETS_FILE, SnippetEntry, SnippetFieldDefinition, SnippetInputKind, SnippetsPlugin,
    load_snippets, save_snippets, snippet_run_action,
};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn load_save_roundtrip() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry {
        alias: "hw".into(),
        text: "hello".into(),
        hide_contents: false,
        prompt_for_fields: false,
        fields: Vec::new(),
        tags: Vec::new(),
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();
    let loaded = load_snippets(SNIPPETS_FILE).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].alias, "hw");
    assert_eq!(loaded[0].text, "hello");
}

#[test]
fn search_returns_typed_snippet_run_action_without_body_payload() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry {
        alias: "hi".into(),
        text: "hello world".into(),
        hide_contents: false,
        prompt_for_fields: false,
        fields: Vec::new(),
        tags: Vec::new(),
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs hi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "hi");
    assert_eq!(results[0].action, snippet_run_action("hi"));
    assert_eq!(results[0].desc, "Snippet");
    assert_eq!(
        parse_action(&results[0]).unwrap(),
        Command::Storage(StorageCommand::SnippetRun("hi".into()))
    );
}

#[test]
fn list_command_returns_entries() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![
        SnippetEntry {
            alias: "a".into(),
            text: "alpha".into(),
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
            tags: Vec::new(),
        },
        SnippetEntry {
            alias: "b".into(),
            text: "beta".into(),
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
            tags: Vec::new(),
        },
    ];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs list");
    assert_eq!(results.len(), 2);
}

#[test]
fn rm_command_returns_remove_actions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry {
        alias: "todelete".into(),
        text: "bye".into(),
        hide_contents: false,
        prompt_for_fields: false,
        fields: Vec::new(),
        tags: Vec::new(),
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs rm todelete");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "snippet:remove:todelete");
}

#[test]
fn search_run_action_keeps_newline_body_out_of_wire_payload() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry {
        alias: "multi".into(),
        text: "a\nb".into(),
        hide_contents: false,
        prompt_for_fields: false,
        fields: Vec::new(),
        tags: Vec::new(),
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs multi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, snippet_run_action("multi"));
    assert_eq!(load_snippets(SNIPPETS_FILE).unwrap()[0].text, "a\nb");
}

#[test]
fn search_add_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs add greet hello world");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "snippet:add:greet|hello world");
}

#[test]
fn launch_action_add_saves_snippet() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_snippets(SNIPPETS_FILE, &[]).unwrap();
    let plugin = SnippetsPlugin::default();
    let action = plugin.search("cs add alias text").remove(0);
    launch_action(&action).unwrap();
    let list = load_snippets(SNIPPETS_FILE).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].alias, "alias");
    assert_eq!(list[0].text, "text");
    assert!(!list[0].hide_contents);
    assert!(!list[0].prompt_for_fields);
    assert!(list[0].fields.is_empty());
}

#[test]
fn command_add_and_inline_edit_preserve_prompt_metadata_and_hidden_flag() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let fields = vec![
        SnippetFieldDefinition {
            name: "name".into(),
            label: "Preferred name".into(),
            default_value: "Ada".into(),
            required: false,
            input_kind: SnippetInputKind::Multiline,
        },
        SnippetFieldDefinition {
            name: "ticket".into(),
            label: "Ticket number".into(),
            default_value: "INC-".into(),
            required: true,
            input_kind: SnippetInputKind::SingleLine,
        },
    ];
    save_snippets(
        SNIPPETS_FILE,
        &[SnippetEntry {
            alias: "hidden".into(),
            text: "Hello {{name}}".into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: fields.clone(),
            tags: vec!["Work".into(), "High Priority".into()],
        }],
    )
    .unwrap();

    let plugin = SnippetsPlugin::default();
    let add_action = plugin
        .search("cs add hidden Updated {{name}} for {{ticket}} by add tag:work")
        .remove(0);
    launch_action(&add_action).unwrap();
    let after_add = load_snippets(SNIPPETS_FILE).unwrap();
    assert_eq!(
        after_add[0].text,
        "Updated {{name}} for {{ticket}} by add tag:work"
    );
    assert!(after_add[0].hide_contents);
    assert!(after_add[0].prompt_for_fields);
    assert_eq!(after_add[0].fields, fields);
    assert_eq!(after_add[0].tags, ["Work", "High Priority"]);

    let edit_action = plugin
        .search("cs edit hidden Inline edit {{name}} ticket {{ticket}} tag:urgent")
        .remove(0);
    launch_action(&edit_action).unwrap();
    let after_edit = load_snippets(SNIPPETS_FILE).unwrap();
    assert_eq!(
        after_edit[0].text,
        "Inline edit {{name}} ticket {{ticket}} tag:urgent"
    );
    assert!(after_edit[0].hide_contents);
    assert!(after_edit[0].prompt_for_fields);
    assert_eq!(after_edit[0].fields, fields);
    assert_eq!(after_edit[0].tags, ["Work", "High Priority"]);
}

#[test]
fn hidden_body_search_and_list_keep_alias_identity_without_leaking_body() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let body = "first needle\r\n秘密 🧪\nlast café";
    save_snippets(
        SNIPPETS_FILE,
        &[SnippetEntry {
            alias: "private-alias".into(),
            text: body.into(),
            hide_contents: true,
            prompt_for_fields: false,
            fields: Vec::new(),
            tags: Vec::new(),
        }],
    )
    .unwrap();
    let plugin = SnippetsPlugin::default();

    for query in ["cs needle", "cs list needle"] {
        let results = plugin.search(query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "private-alias");
        assert_eq!(results[0].action, snippet_run_action("private-alias"));
        assert!(!results[0].action.contains(body));
    }
    assert_eq!(load_snippets(SNIPPETS_FILE).unwrap()[0].text, body);
}

#[test]
fn search_edit_returns_actions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry {
        alias: "greet".into(),
        text: "hello".into(),
        hide_contents: false,
        prompt_for_fields: false,
        fields: Vec::new(),
        tags: Vec::new(),
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs edit");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "snippet:edit:greet");
}

#[test]
fn search_edit_inline_returns_add_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs edit greet hi there");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "snippet:add:greet|hi there");
}

#[test]
fn tags_filter_snippets_across_search_list_remove_and_edit_without_leaking_hidden_bodies() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let make = |alias: &str, text: &str, tags: &[&str], hidden| SnippetEntry {
        alias: alias.into(),
        text: text.into(),
        hide_contents: hidden,
        prompt_for_fields: false,
        fields: Vec::new(),
        tags: tags.iter().map(|tag| (*tag).into()).collect(),
    };
    save_snippets(
        SNIPPETS_FILE,
        &[
            make(
                "private",
                "hidden-only-sensitive-body",
                &["  High   Priority ", "Work"],
                true,
            ),
            make(
                "team",
                r"kind:todo C:\Tools\next.txt",
                &["work", "urgent"],
                false,
            ),
            make("untagged", "ordinary", &[], false),
        ],
    )
    .unwrap();

    let plugin = SnippetsPlugin::default();
    let all = plugin.search("cs list");
    assert_eq!(all.len(), 3, "an empty filter includes untagged snippets");

    let work = plugin.search("cs list tag:work");
    assert_eq!(
        work.iter()
            .map(|result| result.label.as_str())
            .collect::<Vec<_>>(),
        ["private", "team"]
    );

    let both = plugin.search("cs list tag:work tag:urgent");
    assert_eq!(both.len(), 1);
    assert_eq!(both[0].label, "team");

    let multiword = plugin.search(r#"cs list tag:"high priority""#);
    assert_eq!(multiword.len(), 1);
    assert_eq!(multiword[0].label, "private");

    let fuzzy_tag_search = plugin.search("cs priority");
    assert_eq!(fuzzy_tag_search.len(), 1);
    assert_eq!(fuzzy_tag_search[0].action, snippet_run_action("private"));

    let excludes_work = plugin.search("cs list !tag:work");
    assert_eq!(
        excludes_work
            .iter()
            .map(|result| result.label.as_str())
            .collect::<Vec<_>>(),
        ["untagged"]
    );

    let generic = plugin.search("cs tag:work tag:urgent");
    assert_eq!(generic.len(), 1);
    assert_eq!(generic[0].action, snippet_run_action("team"));

    let remove = plugin.search("cs rm tag:work");
    assert_eq!(remove.len(), 2);
    assert!(
        remove
            .iter()
            .all(|result| result.action.starts_with("snippet:remove:"))
    );
    let remove_by_tag_text = plugin.search("cs rm urgent");
    assert_eq!(remove_by_tag_text.len(), 1);
    assert_eq!(remove_by_tag_text[0].action, "snippet:remove:team");

    let edit_selection = plugin.search(r#"cs edit tag:"high priority""#);
    assert_eq!(edit_selection.len(), 1);
    assert_eq!(edit_selection[0].action, "snippet:edit:private");
    let edit_by_tag_text = plugin.search("cs edit urgent");
    assert_eq!(edit_by_tag_text.len(), 1);
    assert_eq!(edit_by_tag_text[0].action, "snippet:edit:team");

    let inline_edit = plugin.search("cs edit private keep this tag:work");
    assert_eq!(
        inline_edit[0].action,
        "snippet:add:private|keep this tag:work"
    );

    let add = plugin.search("cs add new keep tag:work");
    assert_eq!(add[0].action, "snippet:add:new|keep tag:work");

    let path_query = plugin.search(r"cs C:\Tools\next.txt");
    assert_eq!(path_query.len(), 1);
    assert_eq!(path_query[0].action, snippet_run_action("team"));

    let kind_query = plugin.search("cs kind:todo");
    assert_eq!(kind_query.len(), 1);
    assert_eq!(kind_query[0].action, snippet_run_action("team"));

    let sensitive_search = plugin.search("cs hidden-only-sensitive-body");
    assert_eq!(sensitive_search.len(), 1);
    assert_eq!(sensitive_search[0].action, snippet_run_action("private"));
    assert!(
        !sensitive_search[0]
            .action
            .contains("hidden-only-sensitive-body")
    );
}
