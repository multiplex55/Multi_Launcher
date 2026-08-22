use multi_launcher::mkmacro::*;
use std::fs;
use tempfile::tempdir;

fn invalid_doc() -> MkMacroDocument {
    MkMacroDocument {
        settings: Default::default(),
        schema_version: SCHEMA_VERSION,
        macros: vec![MkMacro {
            id: 7,
            name: "recover me".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            playback: Default::default(),
            steps: vec![MkStep {
                id: 8,
                enabled: true,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Else,
            }],
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
        settings: settings.clone(),
        macros: vec![
            MkMacro {
                id: 101,
                name: "First durable macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
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
            },
            MkMacro {
                id: 102,
                name: "Second durable macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![MkStep {
                    id: 202,
                    enabled: true,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: Default::default(),
                    action: MkAction::Delay { milliseconds: 42 },
                }],
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
