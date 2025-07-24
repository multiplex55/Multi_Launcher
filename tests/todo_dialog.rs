use multi_launcher::gui::TodoDialog;
use multi_launcher::plugins::todo::TodoEntry;

#[test]
fn filter_by_text() {
    let entries = vec![
        TodoEntry { text: "alpha".into(), done: false, priority: 0, tags: vec![] },
        TodoEntry { text: "beta".into(), done: false, priority: 0, tags: vec!["x".into()] },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "beta");
    assert_eq!(idx, vec![1]);
}

#[test]
fn filter_by_tag() {
    let entries = vec![
        TodoEntry { text: "alpha".into(), done: false, priority: 0, tags: vec!["rs3".into()] },
        TodoEntry { text: "beta".into(), done: false, priority: 0, tags: vec!["other".into()] },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "#rs3");
    assert_eq!(idx, vec![0]);
}

#[test]
fn empty_filter_returns_all() {
    let entries = vec![
        TodoEntry { text: "one".into(), done: false, priority: 0, tags: vec![] },
        TodoEntry { text: "two".into(), done: false, priority: 0, tags: vec![] },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "");
    assert_eq!(idx, vec![0, 1]);
}
