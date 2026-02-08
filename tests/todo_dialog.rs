use multi_launcher::gui::TodoDialog;
use multi_launcher::plugins::todo::TodoEntry;

#[test]
fn filter_by_text() {
    let entries = vec![
        TodoEntry {
            id: String::new(),
            text: "alpha".into(),
            done: false,
            priority: 0,
            tags: vec![],
            entity_refs: Vec::new(),
        },
        TodoEntry {
            id: String::new(),
            text: "beta".into(),
            done: false,
            priority: 0,
            tags: vec!["x".into()],
            entity_refs: Vec::new(),
        },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "beta");
    assert_eq!(idx, vec![1]);
}

#[test]
fn filter_by_tag() {
    let entries = vec![
        TodoEntry {
            id: String::new(),
            text: "alpha".into(),
            done: false,
            priority: 0,
            tags: vec!["rs3".into()],
            entity_refs: Vec::new(),
        },
        TodoEntry {
            id: String::new(),
            text: "beta".into(),
            done: false,
            priority: 0,
            tags: vec!["other".into()],
            entity_refs: Vec::new(),
        },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "#rs3");
    assert_eq!(idx, vec![0]);
}

#[test]
fn empty_filter_returns_all() {
    let entries = vec![
        TodoEntry {
            id: String::new(),
            text: "one".into(),
            done: false,
            priority: 0,
            tags: vec![],
            entity_refs: Vec::new(),
        },
        TodoEntry {
            id: String::new(),
            text: "two".into(),
            done: false,
            priority: 0,
            tags: vec![],
            entity_refs: Vec::new(),
        },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "");
    assert_eq!(idx, vec![0, 1]);
}

#[test]
fn add_todo_parses_multiple_tags() {
    let mut dlg = TodoDialog::default();
    dlg.test_set_text("task");
    dlg.test_set_tags("alpha, beta, gamma, delta");
    assert!(dlg.test_add_todo());
    assert_eq!(
        dlg.test_entries()[0].tags,
        vec![
            "alpha".to_string(),
            "beta".to_string(),
            "gamma".to_string(),
            "delta".to_string(),
        ]
    );
}

#[test]
fn add_todo_ignores_trailing_comma() {
    let mut dlg = TodoDialog::default();
    dlg.test_set_text("task");
    dlg.test_set_tags("alpha, beta, gamma, delta,");
    assert!(dlg.test_add_todo());
    assert_eq!(
        dlg.test_entries()[0].tags,
        vec![
            "alpha".to_string(),
            "beta".to_string(),
            "gamma".to_string(),
            "delta".to_string(),
        ]
    );
}
