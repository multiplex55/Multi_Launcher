use multi_launcher::gui::MkMacroDialog;
use multi_launcher::mkmacro::*;
use std::fs;
use std::sync::Arc;
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
                breakpoint: false,
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
                    breakpoint: false,
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
                    breakpoint: false,
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
            breakpoint: false,
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
fn schema_eight_migrates_through_store_and_persists_canonical_schema_ten() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(MKMACROS_FILE);
    let fixture = include_str!("fixtures/mkmacros_v8.json");
    fs::write(&path, fixture).unwrap();

    let (store, disposition) = MkMacroStore::open(dir.path()).unwrap();
    assert!(matches!(disposition, LoadDisposition::Loaded));
    let first = (*store.snapshot()).clone();
    assert_eq!(first.schema_version, SCHEMA_VERSION);
    assert_eq!(first.schema_version, 10);
    assert!(first.folders.is_empty());
    for mac in &first.macros {
        assert_eq!(mac.hotkey_scope, MkHotkeyScope::AnyWindow);
        assert_eq!(mac.folder_id, None);
    }
    assert_eq!(
        first.macros[0].steps[0].action,
        MkAction::Delay(MkDelayPayload {
            mode: MkDelayMode::Fixed,
            fixed_ms: 12_345,
            minimum_ms: 0,
            maximum_ms: 12_345,
        })
    );

    // Start with the literal legacy fixture and change only the schema-9 and
    // schema-10 fields.
    // Full JSON equality also protects IDs, ordering, executable content, and
    // nondefault macro/step options from accidental normalization or loss.
    let mut expected: serde_json::Value = serde_json::from_str(fixture).unwrap();
    expected["schema_version"] = serde_json::json!(10);
    expected["folders"] = serde_json::json!([]);
    for mac in expected["macros"].as_array_mut().unwrap() {
        mac["hotkey_scope"] = serde_json::json!({"type": "any_window"});
        mac["folder_id"] = serde_json::Value::Null;
        for step in mac["steps"].as_array_mut().unwrap() {
            step["breakpoint"] = serde_json::Value::Bool(false);
        }
    }
    expected["macros"][0]["steps"][0]["action"]["data"] = serde_json::json!({
        "mode": "fixed", "fixed_ms": 12345, "minimum_ms": 0, "maximum_ms": 12345
    });
    assert_eq!(serde_json::to_value(&first).unwrap(), expected);

    // Opening a legacy document itself persists the migration.
    let after_open: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(after_open, expected);
    assert_eq!(*store.save(first.clone()).unwrap(), first);
    let persisted: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted, expected);
    assert!(
        persisted["macros"][0]["steps"][0]["action"]["data"]
            .get("milliseconds")
            .is_none()
    );

    drop(store);
    let (reopened, disposition) = MkMacroStore::open(dir.path()).unwrap();
    assert!(matches!(disposition, LoadDisposition::Loaded));
    assert_eq!(*reopened.snapshot(), first);
}

#[test]
fn schema_nine_load_adds_breakpoints_and_repairs_only_dangling_folder_membership() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(MKMACROS_FILE);
    let fixture = include_str!("fixtures/mkmacros_v9_dangling_folder.json");
    fs::write(&path, fixture).unwrap();
    let original: MkMacroDocument = serde_json::from_str(fixture).unwrap();
    assert_eq!(original.schema_version, 9);
    assert_eq!(original.macros[1].folder_id, Some(999));
    assert!(!original.folders.iter().any(|folder| folder.id == 999));
    let mut expected = original.clone();
    expected.schema_version = SCHEMA_VERSION;
    expected.macros[1].folder_id = None;

    let (store, disposition) = MkMacroStore::open(dir.path()).unwrap();
    assert!(matches!(disposition, LoadDisposition::Loaded));
    let repaired = store.snapshot();
    assert_eq!(repaired.schema_version, SCHEMA_VERSION);
    assert_eq!(*repaired, expected);
    assert_eq!(repaired.macros[0].folder_id, Some(42));
    assert_eq!(repaired.macros[1].folder_id, None);
    assert_eq!(repaired.macros[2].folder_id, Some(7));
    assert_eq!(repaired.macros[3].folder_id, None);
    assert_eq!(repaired.folders, original.folders);
    assert!(
        repaired
            .macros
            .iter()
            .flat_map(|mac| mac.steps.iter())
            .all(|step| !step.breakpoint)
    );
    assert_eq!(
        repaired.macros.iter().map(|mac| mac.id).collect::<Vec<_>>(),
        original.macros.iter().map(|mac| mac.id).collect::<Vec<_>>()
    );

    // Verify the on-load repair reaches disk without changing anything else,
    // including the deliberately unsorted arrays and the unused folder.
    let mut expected_json: serde_json::Value = serde_json::from_str(fixture).unwrap();
    expected_json["schema_version"] = serde_json::json!(10);
    expected_json["macros"][1]["folder_id"] = serde_json::Value::Null;
    for mac in expected_json["macros"].as_array_mut().unwrap() {
        for step in mac["steps"].as_array_mut().unwrap() {
            step["breakpoint"] = serde_json::Value::Bool(false);
        }
    }
    let persisted: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted, expected_json);
    let persisted_bytes = fs::read(&path).unwrap();
    drop(store);
    let (reopened, disposition) = MkMacroStore::open(dir.path()).unwrap();
    assert!(matches!(disposition, LoadDisposition::Loaded));
    assert_eq!(*reopened.snapshot(), expected);
    assert_eq!(fs::read(&path).unwrap(), persisted_bytes);
}

#[test]
fn schema_nine_normal_documents_load_through_public_store_without_manual_migration() {
    let documents = [
        serde_json::json!({
            "schema_version": 9,
            "macros": []
        }),
        serde_json::json!({
            "schema_version": 9,
            "folders": [],
            "macros": [
                {
                    "id": 10,
                    "name": "mixed actions",
                    "description": "preserve all step data",
                    "enabled": true,
                    "hotkey": null,
                    "hotkey_scope": {"type": "any_window"},
                    "folder_id": null,
                    "playback": {"speed_percent": 110, "random_delay_ms": 4, "random_offset_px": 2},
                    "steps": [
                        {
                            "id": 4,
                            "enabled": true,
                            "repeat": 3,
                            "delay_after_ms": 27,
                            "on_error": "continue",
                            "action": {"type": "image_find", "data": {
                                "asset_id": 21,
                                "wait": {"timeout_ms": 500, "poll_interval_ms": 25},
                                "region": {"type": "desktop"},
                                "tolerance": 8,
                                "alpha": "ignore",
                                "return_point": "top_left",
                                "not_found_policy": "fail",
                                "outputs": {"found": "found", "point": "point", "x": "x", "y": "y"}
                            }}
                        },
                        {
                            "id": 5,
                            "enabled": false,
                            "repeat": 2,
                            "delay_after_ms": 11,
                            "on_error": "stop",
                            "action": {"type": "text", "data": {
                                "text": "disabled but preserved",
                                "mode": "paste"
                            }}
                        }
                    ],
                    "image_assets": []
                },
                {
                    "id": 11,
                    "name": "empty steps",
                    "enabled": false,
                    "steps": []
                }
            ]
        }),
    ];

    for original in documents {
        let dir = tempdir().unwrap();
        let path = dir.path().join(MKMACROS_FILE);
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();

        // The public store API performs the migration as part of opening the document.
        let (store, disposition) = MkMacroStore::open(dir.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        let loaded = store.snapshot();
        assert_eq!(loaded.schema_version, 10);
        assert_eq!(
            loaded.macros.len(),
            original["macros"].as_array().unwrap().len()
        );

        for (macro_index, original_macro) in
            original["macros"].as_array().unwrap().iter().enumerate()
        {
            let loaded_macro = &loaded.macros[macro_index];
            let original_steps = original_macro
                .get("steps")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert_eq!(loaded_macro.steps.len(), original_steps.len());
            for (step_index, original_step) in original_steps.iter().enumerate() {
                assert!(original_step.get("breakpoint").is_none());
                let loaded_step = &loaded_macro.steps[step_index];
                assert_eq!(loaded_step.id, original_step["id"]);
                assert_eq!(loaded_step.enabled, original_step["enabled"]);
                assert_eq!(loaded_step.repeat, original_step["repeat"]);
                assert_eq!(loaded_step.delay_after_ms, original_step["delay_after_ms"]);
                assert_eq!(
                    serde_json::to_value(&loaded_step.on_error).unwrap(),
                    original_step["on_error"]
                );
                assert_eq!(
                    serde_json::to_value(&loaded_step.action).unwrap(),
                    original_step["action"]
                );
                assert!(!loaded_step.breakpoint);
            }
        }

        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], 10);
        for mac in persisted["macros"].as_array().unwrap() {
            for step in mac["steps"].as_array().unwrap() {
                assert_eq!(step["breakpoint"], serde_json::Value::Bool(false));
            }
        }

        let persisted_bytes = fs::read(&path).unwrap();
        drop(store);
        let (reopened, disposition) = MkMacroStore::open(dir.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        assert_eq!(reopened.snapshot().schema_version, 10);
        assert_eq!(fs::read(&path).unwrap(), persisted_bytes);
    }
}

#[test]
fn breakpoint_values_round_trip_through_store_json() {
    let dir = tempdir().unwrap();
    let mut document = invalid_doc();
    document.macros[0].steps[0].breakpoint = true;
    document.macros[0].steps[0].action = MkAction::Text(MkTextPayload {
        text: "persisted breakpoint".into(),
        mode: MkTextMode::Paste,
    });

    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    store.save(document).unwrap();
    let path = dir.path().join(MKMACROS_FILE);
    let persisted: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted["macros"][0]["steps"][0]["breakpoint"], true);
    drop(store);

    let (reopened, disposition) = MkMacroStore::open(dir.path()).unwrap();
    assert!(matches!(disposition, LoadDisposition::Loaded));
    assert!(reopened.snapshot().macros[0].steps[0].breakpoint);
    let reloaded_json = serde_json::to_value(reopened.snapshot().as_ref()).unwrap();
    assert_eq!(reloaded_json["macros"][0]["steps"][0]["breakpoint"], true);
    let persisted_after_reopen: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        persisted_after_reopen["macros"][0]["steps"][0]["breakpoint"],
        true
    );
}

#[test]
fn folder_metadata_round_trips_with_repairs_and_excludes_dialog_ui_state() {
    let dir = tempdir().unwrap();
    let original = MkMacroDocument {
        schema_version: SCHEMA_VERSION,
        settings: Default::default(),
        folders: vec![
            MkMacroFolder {
                id: 42,
                name: "Zebra".into(),
            },
            MkMacroFolder {
                id: 7,
                name: "Alpha".into(),
            },
            MkMacroFolder {
                id: 19,
                name: "Unused folder".into(),
            },
        ],
        macros: vec![
            MkMacro {
                id: 81,
                name: "Valid first folder".into(),
                description: "Keep this reference".into(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: Some(42),
                playback: Default::default(),
                steps: vec![],
                image_assets: vec![],
            },
            MkMacro {
                id: 12,
                name: "Dangling folder".into(),
                description: "Only clear the membership".into(),
                enabled: false,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: Some(999),
                playback: Default::default(),
                steps: vec![],
                image_assets: vec![],
            },
            MkMacro {
                id: 42,
                name: "Valid second folder".into(),
                description: "Keep this membership".into(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: Some(7),
                playback: Default::default(),
                steps: vec![],
                image_assets: vec![],
            },
            MkMacro {
                id: 3,
                name: "Already unfiled".into(),
                description: "Keep unfiled membership".into(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                steps: vec![],
                image_assets: vec![],
            },
        ],
    };
    let mut expected = original.clone();
    expected.macros[1].folder_id = None;

    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    store.save(original).unwrap();
    drop(store);

    let (store, disposition) = MkMacroStore::open(dir.path()).unwrap();
    assert!(matches!(disposition, LoadDisposition::Loaded));
    assert_eq!(*store.snapshot(), expected);

    let mut dialog = MkMacroDialog::new(Arc::new(store));
    dialog.collapsed_folders.insert(42);
    dialog.begin_folder_rename(42);
    dialog.folder_rename_text = "Uncommitted rename".into();
    dialog.request_delete_folder(42);
    dialog.search = "temporary search".into();
    let before_save = dialog.draft.clone();
    dialog.save().unwrap();
    assert_eq!(dialog.draft, before_save);

    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join(MKMACROS_FILE)).unwrap()).unwrap();
    assert_eq!(persisted, serde_json::to_value(&expected).unwrap());

    drop(dialog);
    let (reopened, disposition) = MkMacroStore::open(dir.path()).unwrap();
    assert!(matches!(disposition, LoadDisposition::Loaded));
    assert_eq!(*reopened.snapshot(), expected);
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

#[test]
fn persisted_mkmacros_json_excludes_runtime_debug_state() {
    let dir = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    store.save(MkMacroDocument::default()).unwrap();

    let persisted = fs::read_to_string(dir.path().join(MKMACROS_FILE)).unwrap();
    for field in [
        "run_mode",
        "pause_reason",
        "debug_variables",
        "debug_variables_step_id",
        "debug_snapshot_reason",
        "last_completed_step_id",
    ] {
        assert!(
            !persisted.contains(&format!("\"{field}\"")),
            "runtime-only field {field} leaked into mkmacros.json: {persisted}"
        );
    }
}
