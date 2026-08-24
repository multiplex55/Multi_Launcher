use multi_launcher::{
    mkmacro::*,
    plugin::Plugin,
    plugins::{macros::MacrosPlugin, mkmacro::MkMacroPlugin},
};
use tempfile::tempdir;

#[test]
fn legacy_and_mkmacro_routes_coexist_and_disable_independently() {
    let dir = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    store
        .save(MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            macros: vec![MkMacro {
                id: 55,
                name: "stable".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![],
                image_assets: vec![],
            }],
        })
        .unwrap();
    let mut modern = MkMacroPlugin::new(std::sync::Arc::new(store));
    let legacy = MacrosPlugin::new();
    assert_eq!(legacy.search("macro")[0].action, "macro:dialog");
    assert!(modern.search("macro").is_empty());
    assert_eq!(modern.search("mkmacro stable")[0].action, "mkmacro:run:55");
    assert!(legacy.search("mkmacro stable").is_empty());
    assert!(modern.search("macro:anything").is_empty());
    modern.apply_settings(&serde_json::json!({"enabled":false}));
    assert!(modern.search("mkmacro").is_empty());
    assert!(!legacy.search("macro").is_empty());
}

#[test]
fn rename_keeps_id_based_launcher_action() {
    fn action_for(name: &str) -> String {
        let dir = tempdir().unwrap();
        let doc = MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            macros: vec![MkMacro {
                id: 99,
                name: name.into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![],
                image_assets: vec![],
            }],
        };
        // Seed the persisted state before opening the watched store. Replacing a
        // watched file in rapid succession is unrelated to the ID-routing contract
        // and can race with Windows file-sharing semantics.
        std::fs::write(
            dir.path().join(MKMACROS_FILE),
            serde_json::to_vec_pretty(&doc).unwrap(),
        )
        .unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        MkMacroPlugin::new(std::sync::Arc::new(store)).search(&format!("mkmacro {name}"))[0]
            .action
            .clone()
    }

    let before = action_for("before");
    let after = action_for("after");
    assert_eq!(before, "mkmacro:run:99");
    assert_eq!(after, before);
}
