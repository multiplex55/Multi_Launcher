use multi_launcher::mkmacro::*;
use std::fs;
use tempfile::tempdir;

fn invalid_doc() -> MkMacroDocument {
    MkMacroDocument {
        settings: Default::default(),
        schema_version: SCHEMA_VERSION,
        folders: vec![],
        macros: vec![MkMacro {
            id: 7,
            name: "recover me".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            steps: vec![MkStep {
                id: 8,
                enabled: true,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Else,
            }],
            image_assets: vec![],
        }],
    }
}

#[test]
fn invalid_draft_survives_save_reload_but_is_not_runnable() {
    let dir = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    store.save(invalid_doc()).unwrap();
    assert!(!store.can_run());
    drop(store);
    let (reloaded, _) = MkMacroStore::open(dir.path()).unwrap();
    assert_eq!(reloaded.snapshot().macros[0].name, "recover me");
    assert!(!reloaded.can_run());
    assert!(compile(&reloaded.snapshot().macros[0]).is_err());
}

#[test]
fn legacy_file_is_neither_read_nor_written() {
    let dir = tempdir().unwrap();
    let legacy = dir.path().join("macros.json");
    fs::write(&legacy, br#"[{"label":"legacy","desc":"","steps":[]}]"#).unwrap();
    let before = fs::read(&legacy).unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    assert!(store.snapshot().macros.is_empty());
    store.save(MkMacroDocument::default()).unwrap();
    assert_eq!(fs::read(&legacy).unwrap(), before);
    assert!(dir.path().join(MKMACROS_FILE).exists());
}

#[test]
fn delete_all_is_durable_and_never_falls_back_to_legacy_file() {
    let dir = tempdir().unwrap();
    let legacy_path = dir.path().join("macros.json");
    let legacy_contents = br#"[{"label":"legacy survivor","desc":"do not touch","steps":[]}]"#;
    fs::write(&legacy_path, legacy_contents).unwrap();

    let settings = MkMacroSettings {
        record_toggle_hotkey: MkHotkey {
            key: MkKey::Function(7),
            modifiers: vec![MkKey::Control],
        },
    };
    let document = MkMacroDocument {
        schema_version: SCHEMA_VERSION,
        folders: vec![],
        settings: settings.clone(),
        macros: vec![
            MkMacro {
                id: 101,
                name: "First durable macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                steps: vec![MkStep {
                    id: 201,
                    enabled: true,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: Default::default(),
                    action: MkAction::Text(MkTextPayload {
                        text: "first step".into(),
                        mode: MkTextMode::Type,
                    }),
                }],
                image_assets: vec![],
            },
            MkMacro {
                id: 102,
                name: "Second durable macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                steps: vec![MkStep {
                    id: 202,
                    enabled: true,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: Default::default(),
                    action: MkAction::Delay(MkDelayPayload {
                        fixed_ms: 42,
                        ..Default::default()
                    }),
                }],
                image_assets: vec![],
            },
        ],
    };

    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    store.save(document).unwrap();
    drop(store);

    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    let loaded = store.snapshot();
    assert_eq!(loaded.macros.len(), 2);
    assert!(
        loaded
            .macros
            .iter()
            .any(|item| item.id == 101 && item.name == "First durable macro")
    );
    assert!(
        loaded
            .macros
            .iter()
            .any(|item| item.id == 102 && item.name == "Second durable macro")
    );

    // Mirror the dialog's draft mutation: remove macros from the loaded document,
    // then send that complete document through the public store save API.
    let mut empty_document = (*loaded).clone();
    empty_document.macros.clear();
    assert!(empty_document.macros.is_empty());
    let saved = store.save(empty_document).unwrap();
    assert!(saved.macros.is_empty());
    assert!(store.snapshot().macros.is_empty());

    let canonical_path = dir.path().join(MKMACROS_FILE);
    assert!(canonical_path.exists());
    let raw = fs::read(&canonical_path).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(json["schema_version"], SCHEMA_VERSION);
    assert_eq!(json["settings"], serde_json::to_value(&settings).unwrap());
    assert_eq!(json["macros"].as_array().map(Vec::len), Some(0));
    let text = String::from_utf8(raw).unwrap();
    for stale in ["First durable macro", "Second durable macro", "101", "102"] {
        assert!(!text.contains(stale), "canonical file retained {stale:?}");
    }
    assert_eq!(fs::read(&legacy_path).unwrap(), legacy_contents);

    drop(saved);
    drop(loaded);
    drop(store);
    let (reopened, _) = MkMacroStore::open(dir.path()).unwrap();
    let reopened_snapshot = reopened.snapshot();
    assert!(reopened_snapshot.macros.is_empty());
    assert!(reopened_snapshot.macros.iter().all(|item| {
        ![101, 102].contains(&item.id)
            && !["First durable macro", "Second durable macro"].contains(&item.name.as_str())
    }));
    assert_eq!(fs::read(&legacy_path).unwrap(), legacy_contents);
}

#[test]
fn schema_six_is_normalized_and_delay_is_migrated() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(MKMACROS_FILE);
    let original_actions = serde_json::json!([
        {"type":"delay","data":{"milliseconds":42}},
        {"type":"text","data":{"text":"unchanged","mode":"type"}}
    ]);
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 6,
            "macros": [{
                "id": 1, "name": "legacy", "steps": [
                    {"id": 1, "action": original_actions[0]},
                    {"id": 2, "action": original_actions[1]}
                ]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    assert_eq!(store.snapshot().schema_version, SCHEMA_VERSION);
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let saved_actions: Vec<_> = saved["macros"][0]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["action"].clone())
        .collect();
    assert_eq!(
        saved_actions,
        vec![
            serde_json::json!({
                "type": "delay",
                "data": {
                    "mode": "fixed",
                    "fixed_ms": 42,
                    "minimum_ms": 0,
                    "maximum_ms": 42
                }
            }),
            original_actions[1].clone(),
        ]
    );
}

#[test]
fn schema_seven_new_actions_migrate_and_survive_store_round_trips() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(MKMACROS_FILE);
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 7,
            "macros": [{"id": 1, "name": "new", "steps": [
                {"id": 1, "action": {"type": "notify", "data": {
                    "title": "Ready", "description": "Done", "kind": "success",
                    "duration": "long", "show_symbol": false
                }}},
                {"id": 2, "action": {"type": "play_sound", "data": {"sound": "Alarm.wav"}}}
            ]}]
        }))
        .unwrap(),
    )
    .unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    let first = (*store.snapshot()).clone();
    store.save(first.clone()).unwrap();
    drop(store);
    let (reloaded, _) = MkMacroStore::open(dir.path()).unwrap();
    assert_eq!(*reloaded.snapshot(), first);

    let structural = serde_json::to_value(reloaded.snapshot().as_ref()).unwrap();
    assert_eq!(structural["schema_version"], SCHEMA_VERSION);
    assert_eq!(
        structural["macros"][0]["steps"][0]["action"]["type"],
        "notify"
    );
    assert_eq!(
        structural["macros"][0]["steps"][0]["action"]["data"]["kind"],
        "success"
    );
    assert_eq!(
        structural["macros"][0]["steps"][0]["action"]["data"]["duration"],
        "long"
    );
    assert_eq!(
        structural["macros"][0]["steps"][1]["action"]["type"],
        "play_sound"
    );
    assert_eq!(
        structural["macros"][0]["steps"][1]["action"]["data"]["sound"],
        "Alarm.wav"
    );
}

#[test]
fn schema_seven_notification_sequence_preserves_order_and_payloads() {
    let dir = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    let actions = vec![
        MkAction::SetVariable {
            name: "files_copied".into(),
            value: MkValue::Number(42.0),
        },
        MkAction::SetVariable {
            name: "destination".into(),
            value: MkValue::String(r"D:\Backup".into()),
        },
        MkAction::Notify(MkNotifyPayload {
            title: "Backup complete".into(),
            description: r"Copied ${files_copied} files to ${destination}".into(),
            kind: MkNotificationKind::Success,
            duration: MkNotificationDuration::Long,
            show_symbol: false,
        }),
        MkAction::PlaySound(MkPlaySoundPayload {
            sound: "ReminderStart.wav".into(),
        }),
        MkAction::Text(MkTextPayload {
            text: "observable".into(),
            mode: MkTextMode::Type,
        }),
    ];
    let steps = actions
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, action)| MkStep {
            id: i as u64 + 1,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        })
        .collect();
    store
        .save(MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            folders: vec![],
            settings: Default::default(),
            macros: vec![MkMacro {
                id: 77,
                name: "backup".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                steps,
                image_assets: vec![],
            }],
        })
        .unwrap();
    drop(store);
    let (reopened, _) = MkMacroStore::open(dir.path()).unwrap();
    let snapshot = reopened.snapshot();
    assert_eq!(snapshot.schema_version, SCHEMA_VERSION);
    assert_eq!(
        snapshot.macros[0]
            .steps
            .iter()
            .map(|s| &s.action)
            .collect::<Vec<_>>(),
        actions.iter().collect::<Vec<_>>()
    );
}

#[test]
fn schema_newer_than_current_is_rejected() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(MKMACROS_FILE),
        format!(r#"{{"schema_version":{},"macros":[]}}"#, SCHEMA_VERSION + 1),
    )
    .unwrap();
    let (store, disposition) = MkMacroStore::open(dir.path()).unwrap();
    let expected = format!("newer than supported version {SCHEMA_VERSION}");
    assert!(
        matches!(disposition, LoadDisposition::NeedsUserRecovery { error } if error.contains(&expected))
    );
    assert!(store.snapshot().macros.is_empty());
}
