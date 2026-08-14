use multi_launcher::mkmacro::*;
use std::fs;
use tempfile::tempdir;

fn invalid_doc() -> MkMacroDocument {
    MkMacroDocument {
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
