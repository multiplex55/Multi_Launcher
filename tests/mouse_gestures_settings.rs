use multi_launcher::plugins::mouse_gestures::{
    add_ignore_window_title, normalize_ignore_window_titles,
};

#[test]
fn normalize_ignore_window_titles_dedupes_and_trims() {
    let mut titles = vec![
        "  Notepad  ".to_string(),
        "firefox".to_string(),
        "FIREFOX ".to_string(),
        "".to_string(),
        "   ".to_string(),
        "Terminal".to_string(),
    ];
    let changed = normalize_ignore_window_titles(&mut titles);
    assert!(changed);
    assert_eq!(
        titles,
        vec![
            "Notepad".to_string(),
            "firefox".to_string(),
            "Terminal".to_string()
        ]
    );
}

#[test]
fn add_ignore_window_title_avoids_case_insensitive_duplicates() {
    let mut titles = vec!["Notepad".to_string()];
    assert!(!add_ignore_window_title(&mut titles, "notepad "));
    assert!(add_ignore_window_title(&mut titles, "Firefox"));
    assert_eq!(titles, vec!["Notepad".to_string(), "Firefox".to_string()]);
}
