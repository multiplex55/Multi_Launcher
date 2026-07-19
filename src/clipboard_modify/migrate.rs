use super::config::{
    CURRENT_SCHEMA_VERSION, VersionedClipboardModifiersFile, save_model_atomic, validate_model,
};
use super::model::{ClipboardTemplate, SavedPipeline};
use crate::common::atomic_file::backup_file;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct V0File {
    schema_version: u32,
    #[serde(default)]
    templates: Vec<ClipboardTemplate>,
    #[serde(default)]
    pipelines: Vec<SavedPipeline>,
}

pub fn migrate_file(
    path: &Path,
    version: u32,
    bytes: &[u8],
) -> Result<(
    VersionedClipboardModifiersFile,
    super::model::ClipboardModifierCatalog,
)> {
    match version {
        0 => {
            let old: V0File = serde_json::from_slice(bytes)?;
            anyhow::ensure!(old.schema_version == 0, "v0 migration received schema_version {}", old.schema_version);
            let model = VersionedClipboardModifiersFile {
                schema_version: CURRENT_SCHEMA_VERSION,
                templates: old.templates,
                pipelines: old.pipelines,
            };
            let catalog = validate_model(&model)?;
            let _ = backup_file(path, "schema-migration")?;
            save_model_atomic(path, &model)?;
            Ok((model, catalog))
        }
        _ => anyhow::bail!("unsupported old schema_version {version}"),
    }
}
