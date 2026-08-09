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
