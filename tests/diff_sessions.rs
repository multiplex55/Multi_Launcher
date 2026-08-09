use multi_launcher::diff::persistence::*;

fn session(name: &str) -> SavedDiffSessionV1 {
    SavedDiffSessionV1 {
        name: name.into(),
        left: "left".into(),
        right: "right".into(),
        ..Default::default()
    }
}

#[test]
fn sessions_have_stable_ids_and_explicit_duplicate_names() {
    let mut p = DiffPersistenceV1::default();
    let id = insert_session(&mut p, session("Work")).unwrap();
    assert!(!id.is_empty());
    assert!(matches!(
        insert_session(&mut p, session("work")),
        Err(SessionError::DuplicateName(_))
    ));
    rename_session(&mut p, &id, "Renamed".into()).unwrap();
    assert_eq!(p.named_sessions[0].id, id);
    assert_eq!(delete_session(&mut p, &id).unwrap().name, "Renamed");
}

#[test]
fn session_roundtrip_rules_and_unsupported_data() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("sessions.json");
    let mut p = DiffPersistenceV1::default();
    let mut s = session("rules");
    s.replacement_rules = vec![crate_rule("b"), crate_rule("a")];
    insert_session(&mut p, s).unwrap();
    save(&path, &p).unwrap();
    let loaded = load(&path).unwrap().unwrap();
    assert_eq!(
        loaded.named_sessions[0]
            .replacement_rules
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "a"]
    );
    std::fs::write(&path, r#"{"version":999,"future":"preserve me"}"#).unwrap();
    assert!(matches!(
        load(&path),
        Err(LoadError::UnsupportedVersion(999))
    ));
    assert!(
        std::fs::read_to_string(path)
            .unwrap()
            .contains("preserve me")
    );
}
fn crate_rule(id: &str) -> multi_launcher::diff::settings::ReplacementRuleV1 {
    multi_launcher::diff::settings::ReplacementRuleV1 {
        id: id.into(),
        pattern: "(x)".into(),
        replacement: "$1".into(),
        enabled: true,
    }
}

#[test]
fn recents_are_mode_aware_bounded_clearable_and_separate() {
    let mut p = DiffPersistenceV1::default();
    p.config.max_recent_comparisons = 2;
    insert_session(&mut p, session("named")).unwrap();
    record_recent_mode(&mut p, "a".into(), "b".into(), ComparisonModeV1::Text);
    record_recent_mode(&mut p, "c".into(), "d".into(), ComparisonModeV1::Text);
    record_recent_mode(&mut p, "./a".into(), "./b".into(), ComparisonModeV1::Text);
    assert_eq!(p.recent_comparisons.len(), 2);
    assert_eq!(p.recent_comparisons[0].left, "./a");
    clear_recents(&mut p);
    assert_eq!(p.named_sessions.len(), 1);
}

#[test]
fn failed_atomic_update_preserves_memory_and_file() {
    let d = tempfile::tempdir().unwrap();
    let target = d.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let mut p = DiffPersistenceV1::default();
    assert!(
        update_atomic(&target, &mut p, |state| {
            state.named_sessions.push(session("not committed"));
            Ok(())
        })
        .is_err()
    );
    assert!(p.named_sessions.is_empty());
    assert!(target.is_dir());
}

#[test]
fn old_v1_defaults_new_optional_fields() {
    let json = r#"{"version":1,"config":{},"recent_comparisons":[],"named_sessions":[{"id":"old","name":"old","left":"a","right":"b","pane_split":0.4,"wrap_text":true,"syntax_highlighting":false,"syntax_theme":"old"}],"replacement_rules":[],"unimportant_section_rules":[]}"#;
    let loaded: DiffPersistenceV1 = serde_json::from_str(json).unwrap();
    let session = &loaded.named_sessions[0];
    assert_eq!(session.comparison_mode, ComparisonModeV1::Text);
    assert!(session.case_sensitive);
    assert_eq!(session.folder_display_filter, "all");
    assert_eq!(
        session.content_comparison,
        ContentComparisonModeV1::OnDemand
    );
}

#[test]
fn every_recent_mode_is_recorded_separately() {
    let mut p = DiffPersistenceV1::default();
    for mode in [
        ComparisonModeV1::Text,
        ComparisonModeV1::Folder,
        ComparisonModeV1::Binary,
    ] {
        record_recent_mode(&mut p, "left".into(), "right".into(), mode);
    }
    assert_eq!(p.recent_comparisons.len(), 3);
}

#[test]
fn all_session_durable_fields_roundtrip_without_runtime_state() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("diff.json");
    let mut p = DiffPersistenceV1::default();
    p.window_size = Some([812.0, 612.0]);
    p.window_position = Some([21.0, 34.0]);
    let mut s = session("complete");
    s.comparison_mode = ComparisonModeV1::Binary;
    s.pane_split = 0.37;
    s.wrap_text = true;
    s.syntax_highlighting = false;
    s.syntax_theme = "theme".into();
    s.ignore_whitespace = true;
    s.case_sensitive = false;
    s.replacement_rules = vec![crate_rule("r")];
    s.folder_includes = vec!["*.rs".into()];
    s.folder_excludes = vec!["target".into()];
    s.folder_display_filter = "differences".into();
    s.content_comparison = ContentComparisonModeV1::Always;
    p.named_sessions.push(s.clone());
    save(&path, &p).unwrap();
    let loaded = load(&path).unwrap().unwrap();
    assert_eq!(loaded.named_sessions, vec![s]);
    assert_eq!(loaded.window_size, p.window_size);
    assert_eq!(loaded.window_position, p.window_position);
    let json = std::fs::read_to_string(path).unwrap();
    for forbidden in [
        "computed",
        "scan_handle",
        "progress",
        "watcher",
        "undo",
        "dirty",
        "selection",
        "operation_plan",
    ] {
        assert!(!json.contains(forbidden));
    }
}

#[test]
fn missing_recent_is_a_local_validation_error() {
    let recent = DisplayPathPairV1 {
        left: "missing-left".into(),
        right: "missing-right".into(),
        mode: ComparisonModeV1::Text,
    };
    assert!(matches!(
        reopen_recent(&recent),
        Err(SessionError::InvalidPath(_))
    ));
    let persisted = serde_json::to_string(&DiffPersistenceV1 {
        recent_comparisons: vec![recent],
        ..Default::default()
    })
    .unwrap();
    assert!(serde_json::from_str::<DiffPersistenceV1>(&persisted).is_ok());
}
