use super::defaults::{default_pipelines, default_templates};
use super::model::*;
use crate::common::atomic_file::save_atomic;
use crate::common::config_files::{ConfigFileSpec, resolve_config_path};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_RELATIVE_PATH: &str = "clipboard_modifiers.json";
pub const MAX_CONFIG_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionedClipboardModifiersFile {
    pub schema_version: u32,
    #[serde(default)]
    pub templates: Vec<ClipboardTemplate>,
    #[serde(default)]
    pub pipelines: Vec<SavedPipeline>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    ValidLoaded,
    DefaultsCreated,
    InvalidStartupInMemoryDefaults { diagnostic: String },
    InvalidReloadRetained { diagnostic: String },
    UnsupportedFutureSchema { version: u32 },
    OversizedFile { size: u64 },
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub model: VersionedClipboardModifiersFile,
    pub catalog: ClipboardModifierCatalog,
    pub state: LoadState,
}

pub fn clipboard_config_path(settings_path: &Path) -> PathBuf {
    resolve_config_path(
        settings_path,
        &ConfigFileSpec::new("Clipboard Modify", DEFAULT_RELATIVE_PATH, ""),
    )
}

pub fn default_model() -> VersionedClipboardModifiersFile {
    VersionedClipboardModifiersFile {
        schema_version: CURRENT_SCHEMA_VERSION,
        templates: default_templates(),
        pipelines: default_pipelines(),
    }
}

pub fn serialize_model(model: &VersionedClipboardModifiersFile) -> Result<Vec<u8>> {
    validate_model(model)?;
    let mut s = serde_json::to_string_pretty(model)?;
    s.push('\n');
    Ok(s.into_bytes())
}

pub fn save_model_atomic(path: &Path, model: &VersionedClipboardModifiersFile) -> Result<()> {
    save_atomic(path, &serialize_model(model)?)
}

pub fn load_startup(settings_path: &Path) -> LoadedConfig {
    let path = clipboard_config_path(settings_path);
    if !path.exists() {
        let model = default_model();
        let catalog = validate_model(&model).expect("built-in defaults valid");
        let state = match save_model_atomic(&path, &model) {
            Ok(()) => LoadState::DefaultsCreated,
            Err(e) => LoadState::InvalidStartupInMemoryDefaults {
                diagnostic: e.to_string(),
            },
        };
        return LoadedConfig {
            path,
            model,
            catalog,
            state,
        };
    }
    match load_current_or_migrate(&path) {
        Ok((model, catalog)) => LoadedConfig {
            path,
            model,
            catalog,
            state: LoadState::ValidLoaded,
        },
        Err(LoadError::Future(v)) => {
            let model = default_model();
            let catalog = validate_model(&model).unwrap();
            LoadedConfig {
                path,
                model,
                catalog,
                state: LoadState::UnsupportedFutureSchema { version: v },
            }
        }
        Err(LoadError::Oversized(s)) => {
            let model = default_model();
            let catalog = validate_model(&model).unwrap();
            LoadedConfig {
                path,
                model,
                catalog,
                state: LoadState::OversizedFile { size: s },
            }
        }
        Err(e) => {
            let model = default_model();
            let catalog = validate_model(&model).unwrap();
            LoadedConfig {
                path,
                model,
                catalog,
                state: LoadState::InvalidStartupInMemoryDefaults {
                    diagnostic: e.to_string(),
                },
            }
        }
    }
}

#[derive(Debug)]
pub enum LoadError {
    Io(String),
    Json(String),
    Future(u32),
    Oversized(u64),
    Invalid(String),
}
impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::error::Error for LoadError {}

pub fn load_current_or_migrate(
    path: &Path,
) -> std::result::Result<(VersionedClipboardModifiersFile, ClipboardModifierCatalog), LoadError> {
    let bytes = read_limited(path)?;
    let root: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| LoadError::Json(e.to_string()))?;
    let v = root
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            LoadError::Invalid("missing schema_version; unversioned input is not accepted".into())
        })? as u32;
    if v > CURRENT_SCHEMA_VERSION {
        return Err(LoadError::Future(v));
    }
    if v < CURRENT_SCHEMA_VERSION {
        return super::migrate::migrate_file(path, v, &bytes)
            .map_err(|e| LoadError::Invalid(e.to_string()));
    }
    let model: VersionedClipboardModifiersFile =
        serde_json::from_value(root).map_err(|e| LoadError::Json(e.to_string()))?;
    let catalog = validate_model(&model).map_err(|e| LoadError::Invalid(e.to_string()))?;
    Ok((model, catalog))
}

pub fn read_limited(path: &Path) -> std::result::Result<Vec<u8>, LoadError> {
    let md = std::fs::metadata(path).map_err(|e| LoadError::Io(e.to_string()))?;
    if md.len() > MAX_CONFIG_BYTES {
        return Err(LoadError::Oversized(md.len()));
    }
    std::fs::read(path).map_err(|e| LoadError::Io(e.to_string()))
}

pub fn validate_model(model: &VersionedClipboardModifiersFile) -> Result<ClipboardModifierCatalog> {
    anyhow::ensure!(
        model.schema_version == CURRENT_SCHEMA_VERSION,
        "unsupported schema_version {}",
        model.schema_version
    );
    let mut templates = model.templates.clone();
    let mut pipelines = model.pipelines.clone();
    normalize_all(&mut templates, &mut pipelines)?;
    for t in &templates {
        t.validate()?;
    }
    for p in &pipelines {
        p.validate()?;
        for st in &p.stages {
            if st.operation == OperationId::Template {
                let n = st.arguments.name.as_deref().unwrap_or("");
                if !templates
                    .iter()
                    .any(|t| t.id == n || t.aliases.iter().any(|a| a == n))
                {
                    anyhow::bail!("referenced template {n} not found");
                }
            }
            if st.operation == OperationId::NamedWrap {
                anyhow::bail!("nested saved-pipeline stages are not allowed");
            }
        }
    }
    ClipboardModifierCatalog::new(templates, pipelines).map_err(Into::into)
}

fn normalize_all(t: &mut [ClipboardTemplate], p: &mut [SavedPipeline]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for n in t
        .iter_mut()
        .flat_map(|x| std::iter::once(&mut x.id).chain(x.aliases.iter_mut()))
        .chain(
            p.iter_mut()
                .flat_map(|x| std::iter::once(&mut x.id).chain(x.aliases.iter_mut())),
        )
    {
        *n = super::catalog::normalize_name(n);
        anyhow::ensure!(!n.is_empty(), "empty identity");
        anyhow::ensure!(seen.insert(n.clone()), "duplicate identity {n}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn write(path: &Path, s: &str) {
        std::fs::write(path, s).unwrap();
    }
    fn minimal() -> VersionedClipboardModifiersFile {
        VersionedClipboardModifiersFile {
            schema_version: CURRENT_SCHEMA_VERSION,
            templates: vec![ClipboardTemplate {
                id: "t".into(),
                label: "T".into(),
                aliases: vec![],
                template: "{{clipboard}}".into(),
                processor: None,
            }],
            pipelines: vec![],
        }
    }

    #[test]
    fn config_path_relative_to_custom_settings() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            clipboard_config_path(&d.path().join("nested/settings.json")),
            d.path().join("nested/clipboard_modifiers.json")
        );
    }
    #[test]
    fn defaults_serialize_and_validate() {
        let m = default_model();
        assert!(validate_model(&m).is_ok());
        assert!(
            String::from_utf8(serialize_model(&m).unwrap())
                .unwrap()
                .contains("schema_version")
        );
    }
    #[test]
    fn rejects_unknown_root_fields() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("clipboard_modifiers.json");
        write(
            &p,
            r#"{"schema_version":1,"templates":[],"pipelines":[],"x":1}"#,
        );
        assert!(load_current_or_migrate(&p).is_err());
    }
    #[test]
    fn rejects_unknown_template_pipeline_stage_argument_fields() {
        for body in [
            r#"{"schema_version":1,"templates":[{"id":"t","label":"T","template":"{{clipboard}}","x":1}],"pipelines":[]}"#,
            r#"{"schema_version":1,"templates":[],"pipelines":[{"id":"p","label":"P","x":1}]}"#,
            r#"{"schema_version":1,"templates":[],"pipelines":[{"id":"p","label":"P","stages":[{"operation":"trim-lines","x":1}]}]}"#,
            r#"{"schema_version":1,"templates":[],"pipelines":[{"id":"p","label":"P","stages":[{"operation":"trim-lines","arguments":{"x":1}}]}]}"#,
        ] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("clipboard_modifiers.json");
            write(&p, body);
            assert!(load_current_or_migrate(&p).is_err());
        }
    }
    #[test]
    fn rejects_oversized_before_read() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("clipboard_modifiers.json");
        let f = std::fs::File::create(&p).unwrap();
        f.set_len(MAX_CONFIG_BYTES + 1).unwrap();
        assert!(matches!(
            load_current_or_migrate(&p),
            Err(LoadError::Oversized(_))
        ));
    }
    #[test]
    fn rejects_reserved_duplicate_missing_reference_nested_future() {
        let mut m = minimal();
        m.templates[0].id = "template".into();
        assert!(validate_model(&m).is_err());
        let mut m = minimal();
        m.templates[0].aliases = vec!["t".into()];
        assert!(validate_model(&m).is_err());
        let mut m = minimal();
        m.templates[0].template = "nope".into();
        assert!(validate_model(&m).is_err());
        let mut m = minimal();
        m.pipelines = vec![SavedPipeline {
            id: "p".into(),
            label: "P".into(),
            aliases: vec![],
            stages: vec![StageSpec {
                operation: OperationId::Template,
                arguments: StageArguments {
                    name: Some("missing".into()),
                    ..Default::default()
                },
            }],
        }];
        assert!(validate_model(&m).is_err());
        let mut m = minimal();
        m.pipelines = vec![SavedPipeline {
            id: "p".into(),
            label: "P".into(),
            aliases: vec![],
            stages: vec![StageSpec {
                operation: OperationId::NamedWrap,
                arguments: StageArguments {
                    name: Some("p2".into()),
                    ..Default::default()
                },
            }],
        }];
        assert!(validate_model(&m).is_err());
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("clipboard_modifiers.json");
        write(
            &p,
            r#"{"schema_version":999,"templates":[],"pipelines":[]}"#,
        );
        assert!(matches!(
            load_current_or_migrate(&p),
            Err(LoadError::Future(999))
        ));
    }
    #[test]
    fn migration_backup_and_rewrite() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("clipboard_modifiers.json");
        write(
            &p,
            r#"{"schema_version":0,"templates":[{"id":"t","label":"T","template":"{{clipboard}}"}],"pipelines":[]}"#,
        );
        let _ = load_current_or_migrate(&p).unwrap();
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .contains("\"schema_version\": 1")
        );
        assert!(std::fs::read_dir(d.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .contains("schema-migration")
        }));
    }
    #[test]
    fn invalid_startup_fallback_and_invalid_reload_retains() {
        let d = tempfile::tempdir().unwrap();
        let settings = d.path().join("settings.json");
        let p = d.path().join("clipboard_modifiers.json");
        write(&p, "{}");
        let loaded = load_startup(&settings);
        assert!(matches!(
            loaded.state,
            LoadState::InvalidStartupInMemoryDefaults { .. }
        ));
        let shared = crate::clipboard_modify::store::shared_default_catalog();
        let before = Arc::clone(&shared.read().unwrap());
        assert!(Arc::ptr_eq(&before, &shared.read().unwrap()));
    }
}

pub fn reset_to_defaults_with_backup(path: &Path) -> Result<VersionedClipboardModifiersFile> {
    let _ = crate::common::atomic_file::backup_file(path, "factory-reset")?;
    let model = default_model();
    save_model_atomic(path, &model)?;
    Ok(model)
}

pub fn recovery_replace_with_backup(
    path: &Path,
    model: &VersionedClipboardModifiersFile,
) -> Result<()> {
    validate_model(model).context("recovery replacement model is invalid")?;
    let _ = crate::common::atomic_file::backup_file(path, "automatic-recovery")?;
    save_model_atomic(path, model)
}
