use multi_launcher::clipboard_modify::config::{
    CURRENT_SCHEMA_VERSION, LoadError, LoadState, MAX_CONFIG_BYTES,
    VersionedClipboardModifiersFile, default_model, load_current_or_migrate, load_startup,
    reset_to_defaults_with_backup, save_model_atomic, validate_model,
};
use multi_launcher::clipboard_modify::model::{ClipboardTemplate, SavedPipeline};

fn minimal() -> VersionedClipboardModifiersFile {
    VersionedClipboardModifiersFile {
        schema_version: CURRENT_SCHEMA_VERSION,
        templates: vec![ClipboardTemplate {
            id: "my template".into(),
            label: "My template".into(),
            aliases: vec!["mt".into()],
            template: "[{{clipboard}}]".into(),
            processor: None,
        }],
        pipelines: Vec::new(),
    }
}

#[test]
fn startup_creates_defaults_next_to_explicit_settings_path() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("profile/settings.json");
    let loaded = load_startup(&settings);
    assert_eq!(
        loaded.path,
        dir.path().join("profile/clipboard_modifiers.json")
    );
    assert!(matches!(loaded.state, LoadState::DefaultsCreated));
    assert!(loaded.path.is_file());
    assert_eq!(
        load_current_or_migrate(&loaded.path).unwrap().0,
        loaded.model
    );
}

#[test]
fn size_limit_is_checked_before_json_parsing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clipboard_modifiers.json");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_CONFIG_BYTES + 1)
        .unwrap();
    assert!(
        matches!(load_current_or_migrate(&path), Err(LoadError::Oversized(n)) if n == MAX_CONFIG_BYTES + 1)
    );
}

#[test]
fn validation_normalizes_and_rejects_colliding_identities() {
    let mut model = minimal();
    model.templates[0].id = "  My__Template ".into();
    model.templates.push(ClipboardTemplate {
        id: "other".into(),
        label: "Other".into(),
        aliases: vec!["my-template".into()],
        template: "{{clipboard}}".into(),
        processor: None,
    });
    assert!(
        validate_model(&model)
            .unwrap_err()
            .to_string()
            .contains("duplicate identity")
    );
}

#[test]
fn placeholder_is_exact_and_reserved_and_cross_kind_names_are_rejected() {
    let mut model = minimal();
    model.templates[0].template = "{{ clipboard }}".into();
    assert!(validate_model(&model).is_err());

    let mut model = minimal();
    model.templates[0].id = "template".into();
    assert!(validate_model(&model).is_err());

    let mut model = minimal();
    model.pipelines.push(SavedPipeline {
        id: "mt".into(),
        label: "Conflict".into(),
        aliases: vec![],
        stages: vec![],
    });
    assert!(validate_model(&model).is_err());
}

#[test]
fn unknown_fields_future_versions_and_invalid_startup_are_safe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clipboard_modifiers.json");
    std::fs::write(
        &path,
        r#"{"schema_version":1,"templates":[],"pipelines":[],"surprise":true}"#,
    )
    .unwrap();
    assert!(matches!(
        load_current_or_migrate(&path),
        Err(LoadError::Json(_))
    ));
    std::fs::write(
        &path,
        r#"{"schema_version":99,"templates":[],"pipelines":[]}"#,
    )
    .unwrap();
    assert!(matches!(
        load_current_or_migrate(&path),
        Err(LoadError::Future(99))
    ));
    std::fs::write(&path, "not json").unwrap();
    let loaded = load_startup(&dir.path().join("settings.json"));
    assert!(matches!(
        loaded.state,
        LoadState::InvalidStartupInMemoryDefaults { .. }
    ));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "not json");
}

#[test]
fn migration_and_factory_reset_create_backups() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clipboard_modifiers.json");
    std::fs::write(
        &path,
        r#"{"schema_version":0,"templates":[],"pipelines":[]}"#,
    )
    .unwrap();
    load_current_or_migrate(&path).unwrap();
    assert!(std::fs::read_dir(dir.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .contains("schema-migration")
    }));

    std::fs::write(&path, serde_json::to_vec(&minimal()).unwrap()).unwrap();
    reset_to_defaults_with_backup(&path).unwrap();
    assert!(std::fs::read_dir(dir.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .contains("factory-reset")
    }));
    assert_eq!(load_current_or_migrate(&path).unwrap().0, default_model());
}

#[test]
fn atomic_save_replaces_content_without_leaving_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clipboard_modifiers.json");
    std::fs::write(&path, b"old").unwrap();
    save_model_atomic(&path, &minimal()).unwrap();
    assert_eq!(load_current_or_migrate(&path).unwrap().0, minimal());
    let names = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![std::ffi::OsString::from("clipboard_modifiers.json")]
    );
}

#[test]
fn defaults_are_not_merged_into_valid_user_catalogs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clipboard_modifiers.json");
    save_model_atomic(&path, &minimal()).unwrap();
    let loaded = load_startup(&dir.path().join("settings.json"));
    assert_eq!(loaded.model.templates.len(), 1);
    assert_eq!(loaded.model.templates[0].id, "my template");
    assert_eq!(loaded.catalog.templates[0].id, "my-template");
    assert!(loaded.model.pipelines.is_empty());
}
