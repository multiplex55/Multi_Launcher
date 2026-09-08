//! User-authored macro templates stored as independent portable packages.
//!
//! Templates are loaded only when their UI is opened. They have no watcher or
//! live relationship with either the source macro or instantiated copies.

use super::{
    MkMacroDocument, MkMacroPackage, MkMacroStore, PackageImportPlan, export_package,
    parse_package, plan_package_import,
};
use crate::common::atomic_file::save_atomic;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

pub const MKMACRO_TEMPLATES_FILE: &str = "mkmacro_templates.json";
pub const TEMPLATE_CATALOG_VERSION: u32 = 1;

fn normalized_template_name(name: &str) -> String {
    name.trim().to_lowercase()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MkMacroTemplate {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub package: MkMacroPackage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MkMacroTemplateCatalog {
    pub format_version: u32,
    #[serde(default)]
    pub templates: Vec<MkMacroTemplate>,
}

impl Default for MkMacroTemplateCatalog {
    fn default() -> Self {
        Self {
            format_version: TEMPLATE_CATALOG_VERSION,
            templates: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCatalogProbe {
    Supported,
    Unsupported(u64),
}

impl MkMacroStore {
    pub fn template_catalog_path(&self) -> PathBuf {
        self.data_directory().join(MKMACRO_TEMPLATES_FILE)
    }

    /// Loads templates on demand. Missing and empty files are an empty catalog;
    /// malformed or future-version files are left untouched for recovery.
    pub fn load_template_catalog(&self) -> Result<MkMacroTemplateCatalog> {
        let path = self.template_catalog_path();
        match fs::read(&path) {
            Ok(bytes) if bytes.is_empty() => Ok(MkMacroTemplateCatalog::default()),
            Ok(bytes) => decode_catalog(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(MkMacroTemplateCatalog::default())
            }
            Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
        }
    }

    /// Captures the complete current draft closure, including embedded assets,
    /// and atomically appends an independent template record.
    pub fn save_macro_template(
        &self,
        document: &MkMacroDocument,
        root_id: u64,
        name: &str,
        description: &str,
    ) -> Result<MkMacroTemplateCatalog> {
        let name = name.trim();
        ensure!(!name.is_empty(), "template name cannot be empty");
        let mut catalog = self.load_template_catalog()?;
        let normalized_name = normalized_template_name(name);
        ensure!(
            !catalog
                .templates
                .iter()
                .any(|template| normalized_template_name(&template.name) == normalized_name),
            "a template named \"{name}\" already exists"
        );
        let bytes = export_package(self, document, &[root_id])?;
        let package = parse_package(&bytes)?;
        let id = catalog
            .templates
            .iter()
            .map(|template| template.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .context("template ID space exhausted")?;
        catalog.templates.push(MkMacroTemplate {
            id,
            name: name.to_owned(),
            description: description.trim().to_owned(),
            package,
        });
        persist_catalog(&self.template_catalog_path(), &catalog)?;
        Ok(catalog)
    }
}

pub fn plan_template_instantiation(
    store: &MkMacroStore,
    template: &MkMacroTemplate,
    authoring_draft: &MkMacroDocument,
    authoring_revision: u64,
    expected_persisted: &MkMacroDocument,
) -> Result<PackageImportPlan> {
    let bytes = serde_json::to_vec(&template.package)?;
    let mut plan = plan_package_import(
        store,
        &bytes,
        authoring_draft,
        authoring_revision,
        expected_persisted,
    )?;
    for imported_id in plan.macro_id_map.values() {
        if let Some(macro_) = plan
            .candidate
            .macros
            .iter_mut()
            .find(|macro_| macro_.id == *imported_id)
        {
            macro_.hotkey = None;
        }
    }
    plan.summary.warnings.push(
        "Template copies have all macro hotkeys cleared to avoid installing conflicts.".into(),
    );
    Ok(plan)
}

pub fn probe_template_catalog(bytes: &[u8]) -> Result<TemplateCatalogProbe> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let version = value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .context("missing template catalog format_version")?;
    if version > TEMPLATE_CATALOG_VERSION as u64 {
        return Ok(TemplateCatalogProbe::Unsupported(version));
    }
    ensure!(
        version == TEMPLATE_CATALOG_VERSION as u64,
        "unsupported template catalog format version {version}"
    );
    decode_catalog(bytes)?;
    Ok(TemplateCatalogProbe::Supported)
}

fn decode_catalog(bytes: &[u8]) -> Result<MkMacroTemplateCatalog> {
    match probe_template_catalog_shape(bytes)? {
        TemplateCatalogProbe::Unsupported(version) => {
            anyhow::bail!("unsupported template catalog format version {version}")
        }
        TemplateCatalogProbe::Supported => {}
    }
    let source: serde_json::Value = serde_json::from_slice(bytes)?;
    let mut catalog: MkMacroTemplateCatalog = serde_json::from_value(source.clone())?;
    let canonical = serde_json::to_value(&catalog)?;
    ensure!(
        source == canonical,
        "template catalog contains unknown fields or a non-canonical nested package shape"
    );
    ensure!(
        catalog.format_version == TEMPLATE_CATALOG_VERSION,
        "unsupported template catalog format version {}",
        catalog.format_version
    );
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for template in &mut catalog.templates {
        ensure!(
            template.id != 0 && ids.insert(template.id),
            "duplicate template ID"
        );
        let name = template.name.trim();
        ensure!(!name.is_empty(), "template name cannot be empty");
        ensure!(
            names.insert(normalized_template_name(name)),
            "duplicate template name"
        );
        template.package = parse_package(&serde_json::to_vec(&template.package)?)?;
    }
    Ok(catalog)
}

fn probe_template_catalog_shape(bytes: &[u8]) -> Result<TemplateCatalogProbe> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let version = value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .context("missing template catalog format_version")?;
    if version > TEMPLATE_CATALOG_VERSION as u64 {
        Ok(TemplateCatalogProbe::Unsupported(version))
    } else {
        ensure!(
            version == TEMPLATE_CATALOG_VERSION as u64,
            "unsupported template catalog format version {version}"
        );
        Ok(TemplateCatalogProbe::Supported)
    }
}

fn persist_catalog(path: &std::path::Path, catalog: &MkMacroTemplateCatalog) -> Result<()> {
    save_atomic(path, &serde_json::to_vec_pretty(catalog)?)
        .with_context(|| format!("save {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkAction, MkHotkey, MkKey, MkMacro, MkMacroDocument, MkStep};

    fn document() -> MkMacroDocument {
        let dependency = MkMacro {
            id: 2,
            name: "Dependency".into(),
            description: String::new(),
            enabled: true,
            hotkey: Some(MkHotkey {
                key: MkKey::Character("a".into()),
                modifiers: vec![],
            }),
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: vec![],
        };
        let root = MkMacro {
            id: 1,
            name: "Root".into(),
            description: String::new(),
            enabled: true,
            hotkey: Some(MkHotkey {
                key: MkKey::Character("b".into()),
                modifiers: vec![],
            }),
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: vec![MkStep {
                id: 1,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                metadata: Default::default(),
                action: MkAction::CallMacro(crate::mkmacro::MkCallMacroPayload {
                    macro_id: 2,
                    ..Default::default()
                }),
            }],
        };
        MkMacroDocument {
            macros: vec![root, dependency],
            ..Default::default()
        }
    }

    #[test]
    fn templates_survive_reload_and_capture_the_full_draft_closure() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let source = document();
        let saved = store
            .save_macro_template(&source, 1, "Reusable root", "Description")
            .unwrap();
        assert_eq!(saved.templates[0].package.manifest.macros.len(), 2);
        let loaded = store.load_template_catalog().unwrap();
        assert_eq!(loaded, saved);
        assert_eq!(*store.snapshot(), MkMacroDocument::default());
    }

    #[test]
    fn unicode_case_collision_is_rejected_without_changing_catalog_or_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let source = document();
        let catalog = store
            .save_macro_template(&source, 1, "Straße", "Original")
            .unwrap();
        let path = store.template_catalog_path();
        let bytes = fs::read(&path).unwrap();

        let error = store
            .save_macro_template(&source, 1, "STRAẞE", "Collision")
            .unwrap_err();

        assert!(error.to_string().contains("already exists"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(store.load_template_catalog().unwrap(), catalog);
    }

    #[test]
    fn template_instances_are_fresh_independent_and_clear_every_hotkey() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let source = document();
        let catalog = store
            .save_macro_template(&source, 1, "Reusable root", "")
            .unwrap();
        let baseline = store.snapshot();
        let first =
            plan_template_instantiation(&store, &catalog.templates[0], &baseline, 0, &baseline)
                .unwrap();
        assert!(
            first
                .candidate()
                .macros
                .iter()
                .all(|macro_| macro_.hotkey.is_none())
        );
        let first_snapshot = first.apply(&store, &baseline, 0).unwrap();
        let mut edited_instance = (*first_snapshot).clone();
        edited_instance.macros[0].name = "Edited instance".into();
        edited_instance.macros[1].steps.push(MkStep {
            id: 99,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            metadata: Default::default(),
            action: MkAction::Delay(Default::default()),
        });
        let edited_instance = store.save(edited_instance).unwrap();
        assert_eq!(
            catalog,
            store.load_template_catalog().unwrap(),
            "editing an instance must not mutate its source template"
        );
        let second = plan_template_instantiation(
            &store,
            &catalog.templates[0],
            &edited_instance,
            1,
            &edited_instance,
        )
        .unwrap();
        let second_snapshot = second.apply(&store, &edited_instance, 1).unwrap();
        assert_eq!(second_snapshot.macros.len(), 4);
        let ids = second_snapshot
            .macros
            .iter()
            .map(|m| m.id)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), 4);
        assert!(
            second_snapshot
                .macros
                .iter()
                .all(|macro_| macro_.hotkey.is_none())
        );
        let second_root = second_snapshot
            .macros
            .iter()
            .find(|macro_| second.imported_root_ids.contains(&macro_.id))
            .unwrap();
        assert_eq!(second_root.name, "Root");
        assert_ne!(second_root.name, "Edited instance");
        assert_eq!(catalog, store.load_template_catalog().unwrap());
    }

    #[test]
    fn catalog_probe_reports_future_versions_without_rewriting() {
        let bytes = br#"{"format_version":99,"templates":[]}"#;
        assert_eq!(
            probe_template_catalog(bytes).unwrap(),
            TemplateCatalogProbe::Unsupported(99)
        );
    }

    #[test]
    fn probe_and_load_reject_unknown_nested_package_fields_without_rewriting() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save_macro_template(&document(), 1, "Strict package", "")
            .unwrap();
        let path = store.template_catalog_path();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["templates"][0]["package"]["manifest"]["macros"][0]["steps"][0]["action"]
            .as_object_mut()
            .unwrap()
            .insert("unknown_nested_field".into(), serde_json::json!(true));
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        fs::write(&path, &bytes).unwrap();

        assert!(probe_template_catalog(&bytes).is_err());
        assert!(store.load_template_catalog().is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
