use multi_launcher::launcher::launch_action;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::snippets::{
    SNIPPETS_FILE, SnippetEntry, SnippetsPlugin, load_snippets, save_snippets,
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
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();
    let loaded = load_snippets(SNIPPETS_FILE).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].alias, "hw");
    assert_eq!(loaded[0].text, "hello");
}

#[test]
fn search_returns_clipboard_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry {
        alias: "hi".into(),
        text: "hello world".into(),
        hide_contents: false,
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs hi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "hi");
    assert_eq!(results[0].action, "clipboard:hello world");
    assert_eq!(results[0].desc, "Snippet");
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
        },
        SnippetEntry {
            alias: "b".into(),
            text: "beta".into(),
            hide_contents: false,
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
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs rm todelete");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "snippet:remove:todelete");
}

#[test]
fn search_preserves_newlines() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry {
        alias: "multi".into(),
        text: "a\nb".into(),
        hide_contents: false,
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs multi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "clipboard:a\nb");
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
}

#[test]
fn command_add_and_inline_edit_preserve_hidden_flag() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_snippets(
        SNIPPETS_FILE,
        &[SnippetEntry {
            alias: "hidden".into(),
            text: "original body".into(),
            hide_contents: true,
        }],
    )
    .unwrap();

    let plugin = SnippetsPlugin::default();
    let add_action = plugin.search("cs add hidden updated by add").remove(0);
    launch_action(&add_action).unwrap();
    let after_add = load_snippets(SNIPPETS_FILE).unwrap();
    assert_eq!(after_add[0].text, "updated by add");
    assert!(after_add[0].hide_contents);

    let edit_action = plugin
        .search("cs edit hidden updated by inline edit")
        .remove(0);
    launch_action(&edit_action).unwrap();
    let after_edit = load_snippets(SNIPPETS_FILE).unwrap();
    assert_eq!(after_edit[0].text, "updated by inline edit");
    assert!(after_edit[0].hide_contents);
}

#[test]
fn hidden_body_search_and_list_keep_alias_label_and_original_clipboard_payload() {
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
        }],
    )
    .unwrap();
    let plugin = SnippetsPlugin::default();

    for query in ["cs needle", "cs list needle"] {
        let results = plugin.search(query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "private-alias");
        assert_eq!(results[0].action, format!("clipboard:{body}"));
        assert_ne!(results[0].action, "clipboard:******");
    }
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
