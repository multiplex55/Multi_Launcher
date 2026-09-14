use super::migration::{DocumentDecodeError, decode_document};
use super::model::{
    AssetId, CURRENT_SCHEMA_VERSION, ConfigRevision, MediaReference, MenuId,
    RADIAL_ASSETS_DIRECTORY, RADIAL_FILE, RadialDocument, SkinId,
};
use super::package::{ImportPlan, PackageError, sha256_hex};
use super::validation::{ValidationErrors, validate};
use crate::common::persistence::{LoadState, PersistenceError, read_bytes, save_json_atomic};
use crate::platform::app_data::AppDataRoot;
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateState {
    Missing,
    Empty,
}

#[derive(Debug)]
pub enum StoreError {
    Candidate(CandidateState),
    Persistence(PersistenceError),
    Malformed {
        path: PathBuf,
        source: serde_json::Error,
    },
    UnsupportedNewerVersion {
        found: u64,
        supported: u32,
    },
    Validation(ValidationErrors),
    RevisionConflict {
        expected: ConfigRevision,
        actual: ConfigRevision,
    },
    RevisionOverflow {
        revision: ConfigRevision,
    },
    LockPoisoned,
    Package(PackageError),
    DiskChanged,
    BackupRequired,
    InvalidBackup,
    ApplyIo {
        path: PathBuf,
        source: std::io::Error,
    },
    ApplyCancelled(ApplyStep),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Candidate(state) => write!(f, "radial store is {state:?}"),
            Self::Persistence(error) => error.fmt(f),
            Self::Malformed { path, source } => write!(
                f,
                "malformed radial document at {}: {source}",
                path.display()
            ),
            Self::UnsupportedNewerVersion { found, supported } => write!(
                f,
                "radial schema {found} is newer than supported schema {supported}"
            ),
            Self::Validation(error) => error.fmt(f),
            Self::RevisionConflict { expected, actual } => write!(
                f,
                "radial revision conflict: expected {}, current {}",
                expected.0, actual.0
            ),
            Self::RevisionOverflow { revision } => {
                write!(f, "radial revision {} cannot be incremented", revision.0)
            }
            Self::LockPoisoned => f.write_str("radial snapshot lock is poisoned"),
            Self::Package(error) => error.fmt(f),
            Self::DiskChanged => f.write_str("radial disk bytes changed since package preview"),
            Self::BackupRequired => {
                f.write_str("explicit replacement requires a verified backup receipt")
            }
            Self::InvalidBackup => f.write_str(
                "replacement backup receipt is missing or does not match current radial bytes",
            ),
            Self::ApplyIo { path, source } => {
                write!(f, "package apply failed at {}: {source}", path.display())
            }
            Self::ApplyCancelled(step) => write!(f, "package apply cancelled at {step:?}"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<PersistenceError> for StoreError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}
impl From<ValidationErrors> for StoreError {
    fn from(value: ValidationErrors) -> Self {
        Self::Validation(value)
    }
}
impl From<PackageError> for StoreError {
    fn from(value: PackageError) -> Self {
        Self::Package(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceImpact {
    pub paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyStep {
    BeforeAssets,
    AfterAsset,
    BeforeDocument,
}

pub trait PackageApplyHook {
    fn checkpoint(&mut self, _step: ApplyStep) -> bool {
        true
    }
}

impl PackageApplyHook for () {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupReceipt {
    pub path: PathBuf,
    pub source_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageApplyDecision {
    CreateNew,
    Replace { backup: BackupReceipt },
}

pub struct PackageApplyRequest {
    pub expected_revision: ConfigRevision,
    pub expected_disk_sha256: Option<String>,
    /// Asset paths observed during preview and their exact content digests.
    pub expected_asset_sha256: std::collections::BTreeMap<PathBuf, String>,
    pub decision: PackageApplyDecision,
    pub plan: ImportPlan,
}

pub struct RadialStore {
    path: PathBuf,
    published: RwLock<Arc<RadialDocument>>,
    transaction: Mutex<()>,
    disk_identity: Mutex<Option<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExternalReloadOutcome {
    Unchanged,
    Published(Arc<RadialDocument>),
}

impl RadialStore {
    pub fn new(root: &AppDataRoot) -> Result<Self, StoreError> {
        Self::at_path(root.path().join(RADIAL_FILE), RadialDocument::starter())
    }
    pub fn at_path(path: impl Into<PathBuf>, initial: RadialDocument) -> Result<Self, StoreError> {
        validate(&initial)?;
        Ok(Self {
            path: path.into(),
            published: RwLock::new(Arc::new(initial)),
            transaction: Mutex::new(()),
            disk_identity: Mutex::new(None),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn snapshot(&self) -> Result<Arc<RadialDocument>, StoreError> {
        self.published
            .read()
            .map(|v| Arc::clone(&v))
            .map_err(|_| StoreError::LockPoisoned)
    }

    /// Load and publish only a fully parsed and validated candidate.
    pub fn reload(&self) -> Result<Arc<RadialDocument>, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let bytes = match read_bytes(&self.path)? {
            LoadState::Missing => return Err(StoreError::Candidate(CandidateState::Missing)),
            LoadState::Empty => return Err(StoreError::Candidate(CandidateState::Empty)),
            LoadState::Loaded(bytes) => bytes,
        };
        let candidate = decode_document(&bytes).map_err(|error| match error {
            DocumentDecodeError::Malformed(source) => StoreError::Malformed {
                path: self.path.clone(),
                source,
            },
            DocumentDecodeError::UnsupportedNewerVersion { found, supported } => {
                StoreError::UnsupportedNewerVersion { found, supported }
            }
            DocumentDecodeError::Validation(error) => StoreError::Validation(error),
        })?;
        let candidate = candidate.document;
        let candidate = Arc::new(candidate);
        let mut published = self
            .published
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut disk_identity = self
            .disk_identity
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        *published = Arc::clone(&candidate);
        *disk_identity = Some(sha256_hex(&bytes));
        Ok(candidate)
    }

    /// Validate, atomically persist, then publish a single monotonic revision.
    pub fn save(
        &self,
        expected: ConfigRevision,
        mut candidate: RadialDocument,
    ) -> Result<Arc<RadialDocument>, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let actual = self.snapshot()?.revision;
        if actual != expected {
            return Err(StoreError::RevisionConflict { expected, actual });
        }
        candidate.schema_version = CURRENT_SCHEMA_VERSION;
        candidate.revision = ConfigRevision(
            actual
                .0
                .checked_add(1)
                .ok_or(StoreError::RevisionOverflow { revision: actual })?,
        );
        validate(&candidate)?;
        let saved_identity =
            sha256_hex(&serde_json::to_vec_pretty(&candidate).map_err(|source| {
                PersistenceError::SerializeJson {
                    path: self.path.clone(),
                    source,
                }
            })?);
        let mut published = self
            .published
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut disk_identity = self
            .disk_identity
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        save_json_atomic(&self.path, &candidate)?;
        let candidate = Arc::new(candidate);
        *published = Arc::clone(&candidate);
        *disk_identity = Some(saved_identity);
        Ok(candidate)
    }

    /// Decode, migrate and validate externally supplied bytes before allowing
    /// the main owner to close active UI and publish one monotonic generation.
    pub fn reload_external_with(
        &self,
        before_publish: impl FnOnce(&RadialDocument),
    ) -> Result<ExternalReloadOutcome, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let bytes = match read_bytes(&self.path)? {
            LoadState::Missing => return Err(StoreError::Candidate(CandidateState::Missing)),
            LoadState::Empty => return Err(StoreError::Candidate(CandidateState::Empty)),
            LoadState::Loaded(bytes) => bytes,
        };
        let identity = sha256_hex(&bytes);
        if self
            .disk_identity
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?
            .as_ref()
            == Some(&identity)
        {
            return Ok(ExternalReloadOutcome::Unchanged);
        }
        let decoded = decode_document(&bytes).map_err(|error| match error {
            DocumentDecodeError::Malformed(source) => StoreError::Malformed {
                path: self.path.clone(),
                source,
            },
            DocumentDecodeError::UnsupportedNewerVersion { found, supported } => {
                StoreError::UnsupportedNewerVersion { found, supported }
            }
            DocumentDecodeError::Validation(error) => StoreError::Validation(error),
        })?;
        let current = self
            .published
            .read()
            .map_err(|_| StoreError::LockPoisoned)?
            .clone();
        let mut candidate = decoded.document;
        let mut comparable = candidate.clone();
        comparable.revision = current.revision;
        if comparable == *current {
            *self
                .disk_identity
                .lock()
                .map_err(|_| StoreError::LockPoisoned)? = Some(identity);
            return Ok(ExternalReloadOutcome::Unchanged);
        }
        candidate.schema_version = CURRENT_SCHEMA_VERSION;
        candidate.revision = ConfigRevision(current.revision.0.checked_add(1).ok_or(
            StoreError::RevisionOverflow {
                revision: current.revision,
            },
        )?);
        validate(&candidate)?;
        before_publish(&candidate);
        let candidate = Arc::new(candidate);
        *self
            .published
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::clone(&candidate);
        *self
            .disk_identity
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(identity);
        Ok(ExternalReloadOutcome::Published(candidate))
    }

    pub fn apply_package(
        &self,
        request: PackageApplyRequest,
    ) -> Result<Arc<RadialDocument>, StoreError> {
        self.apply_package_with_hook(request, &mut ())
    }

    /// Production service boundary for a reviewed legacy preview. The same
    /// package transaction, revision checks, backup policy, and media validation
    /// are used as portable imports; preview parsing itself remains read-only.
    pub fn apply_legacy_import(
        &self,
        preview: &super::import::ImportPreview,
        expected_revision: ConfigRevision,
        expected_disk_sha256: Option<String>,
        decision: PackageApplyDecision,
    ) -> Result<Arc<RadialDocument>, StoreError> {
        self.apply_package(PackageApplyRequest {
            expected_revision,
            expected_disk_sha256,
            expected_asset_sha256: Default::default(),
            decision,
            plan: preview.apply_plan(),
        })
    }

    /// Apply an already validated package plan while holding the store's sole
    /// mutation lock. Assets are immutable/content-addressed and published
    /// before the atomic document commit. Only files created by this call are
    /// removed if a pre-commit step fails.
    pub fn apply_package_with_hook(
        &self,
        request: PackageApplyRequest,
        hook: &mut impl PackageApplyHook,
    ) -> Result<Arc<RadialDocument>, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let current = self
            .published
            .read()
            .map_err(|_| StoreError::LockPoisoned)?
            .clone();
        if current.revision != request.expected_revision {
            return Err(StoreError::RevisionConflict {
                expected: request.expected_revision,
                actual: current.revision,
            });
        }
        let disk_bytes = match read_bytes(&self.path)? {
            LoadState::Loaded(bytes) => bytes,
            LoadState::Missing => Vec::new(),
            LoadState::Empty => Vec::new(),
        };
        if request
            .expected_disk_sha256
            .as_ref()
            .is_some_and(|expected| expected != &sha256_hex(&disk_bytes))
        {
            return Err(StoreError::DiskChanged);
        }
        for (path, expected) in &request.expected_asset_sha256 {
            let bytes = std::fs::read(path).map_err(|source| StoreError::ApplyIo {
                path: path.clone(),
                source,
            })?;
            if sha256_hex(&bytes) != expected.as_str() {
                return Err(StoreError::DiskChanged);
            }
        }
        if let PackageApplyDecision::Replace { backup } = &request.decision {
            let backup_bytes =
                std::fs::read(&backup.path).map_err(|source| StoreError::ApplyIo {
                    path: backup.path.clone(),
                    source,
                })?;
            let backup_is_source = std::fs::canonicalize(&backup.path)
                .ok()
                .zip(std::fs::canonicalize(&self.path).ok())
                .is_some_and(|(backup, source)| backup == source);
            if backup_is_source
                || backup.source_sha256 != sha256_hex(&disk_bytes)
                || sha256_hex(&backup_bytes) != backup.source_sha256
            {
                return Err(StoreError::InvalidBackup);
            }
        }

        let imported_document = request.plan.document;
        let imported_asset_ids = imported_document
            .assets
            .iter()
            .map(|asset| asset.id.clone())
            .collect::<BTreeSet<_>>();
        let package_assets = request.plan.assets;
        for asset in &imported_document.assets {
            let bytes = package_assets
                .values()
                .find(|bytes| {
                    sha256_hex(bytes) == asset.content_sha256
                        && bytes.len() as u64 == asset.byte_len
                })
                .ok_or_else(|| StoreError::Package(PackageError::MissingAsset(asset.id.clone())))?;
            super::assets::validate_packaged_media(bytes, asset.kind).map_err(|reason| {
                StoreError::Package(PackageError::InvalidMedia {
                    asset: asset.id.clone(),
                    reason: reason.to_string(),
                })
            })?;
        }
        let mut candidate = match request.decision {
            PackageApplyDecision::CreateNew => merge_package(&current, imported_document),
            PackageApplyDecision::Replace { .. } => imported_document,
        };
        candidate.schema_version = CURRENT_SCHEMA_VERSION;
        candidate.revision = ConfigRevision(current.revision.0.checked_add(1).ok_or(
            StoreError::RevisionOverflow {
                revision: current.revision,
            },
        )?);
        let asset_root = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(RADIAL_ASSETS_DIRECTORY);
        reject_reparse_asset_root(&asset_root)?;
        let mut publications = Vec::new();
        for asset in &mut candidate.assets {
            if !imported_asset_ids.contains(&asset.id) {
                continue;
            }
            let extension = Path::new(&asset.relative_path)
                .extension()
                .and_then(|value| value.to_str())
                .filter(|value| {
                    value
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
                })
                .map(|value| format!(".{}", value.to_ascii_lowercase()))
                .unwrap_or_default();
            let relative = format!("{}{}", asset.content_sha256, extension);
            if let Some((_, bytes)) = package_assets.iter().find(|(path, bytes)| {
                path.starts_with("assets/")
                    && sha256_hex(bytes) == asset.content_sha256
                    && bytes.len() as u64 == asset.byte_len
            }) {
                publications.push((asset_root.join(&relative), bytes.clone()));
                asset.relative_path = relative;
            }
        }
        validate(&candidate)?;
        if !hook.checkpoint(ApplyStep::BeforeAssets) {
            return Err(StoreError::ApplyCancelled(ApplyStep::BeforeAssets));
        }
        let mut created = Vec::new();
        let result = (|| {
            if !publications.is_empty() {
                std::fs::create_dir_all(&asset_root).map_err(|source| StoreError::ApplyIo {
                    path: asset_root.clone(),
                    source,
                })?;
            }
            for (path, bytes) in publications {
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(mut file) => {
                        if let Err(source) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
                            let _ = std::fs::remove_file(&path);
                            return Err(StoreError::ApplyIo { path, source });
                        }
                        created.push(path);
                    }
                    Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                        let existing =
                            std::fs::read(&path).map_err(|source| StoreError::ApplyIo {
                                path: path.clone(),
                                source,
                            })?;
                        if sha256_hex(&existing) != sha256_hex(&bytes) {
                            return Err(StoreError::DiskChanged);
                        }
                    }
                    Err(source) => return Err(StoreError::ApplyIo { path, source }),
                }
                if !hook.checkpoint(ApplyStep::AfterAsset) {
                    return Err(StoreError::ApplyCancelled(ApplyStep::AfterAsset));
                }
            }
            if !hook.checkpoint(ApplyStep::BeforeDocument) {
                return Err(StoreError::ApplyCancelled(ApplyStep::BeforeDocument));
            }
            let saved_identity =
                sha256_hex(&serde_json::to_vec_pretty(&candidate).map_err(|source| {
                    PersistenceError::SerializeJson {
                        path: self.path.clone(),
                        source,
                    }
                })?);
            save_json_atomic(&self.path, &candidate)?;
            *self
                .disk_identity
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(saved_identity);
            Ok(())
        })();
        if let Err(error) = result {
            for path in created.iter().rev() {
                let _ = std::fs::remove_file(path);
            }
            return Err(error);
        }
        let candidate = Arc::new(candidate);
        *self
            .published
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::clone(&candidate);
        Ok(candidate)
    }
}

fn reject_reparse_asset_root(path: &Path) -> Result<(), StoreError> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    #[cfg(windows)]
    use std::os::windows::fs::MetadataExt;
    #[cfg(windows)]
    let attributes = metadata.file_attributes();
    #[cfg(not(windows))]
    let attributes = 0;
    validate_asset_root_kind(
        path,
        metadata.file_type().is_symlink(),
        metadata.is_dir(),
        attributes,
    )
}

fn validate_asset_root_kind(
    path: &Path,
    symlink: bool,
    directory: bool,
    attributes: u32,
) -> Result<(), StoreError> {
    if symlink {
        return Err(StoreError::ApplyIo {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "radial asset root must not be a symbolic link or junction",
            ),
        });
    }
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(StoreError::ApplyIo {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "radial asset root must not be a reparse point",
            ),
        });
    }
    if !directory {
        return Err(StoreError::ApplyIo {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "radial asset root is not a directory",
            ),
        });
    }
    Ok(())
}

fn merge_package(current: &RadialDocument, imported: RadialDocument) -> RadialDocument {
    let mut merged = current.clone();
    merged.menus.extend(imported.menus);
    merged.skins.extend(imported.skins);
    merged.assets.extend(imported.assets);
    merged.context_rules.extend(imported.context_rules);
    merged.custom_triggers.extend(imported.custom_triggers);
    merged
}

pub fn references_to_menu(document: &RadialDocument, menu_id: &MenuId) -> ReferenceImpact {
    let mut paths = Vec::new();
    if &document.default_menu_id == menu_id {
        paths.push("default_menu_id".into());
    }
    for (mi, menu) in document.menus.iter().enumerate() {
        for (ri, ring) in menu.rings.iter().enumerate() {
            for (ci, cell) in ring.cells.iter().enumerate() {
                if matches!(&cell.content, super::model::CellContent::Submenu { menu_id: target } if target == menu_id)
                {
                    paths.push(format!("menus[{mi}].rings[{ri}].cells[{ci}].content"));
                }
            }
        }
    }
    for (index, rule) in document.context_rules.iter().enumerate() {
        if &rule.menu_id == menu_id {
            paths.push(format!("context_rules[{index}].menu_id"));
        }
    }
    for (index, trigger) in document.custom_triggers.iter().enumerate() {
        if &trigger.menu_id == menu_id {
            paths.push(format!("custom_triggers[{index}].menu_id"));
        }
    }
    ReferenceImpact { paths }
}
pub fn references_to_skin(document: &RadialDocument, skin_id: &SkinId) -> ReferenceImpact {
    ReferenceImpact {
        paths: document
            .menus
            .iter()
            .enumerate()
            .filter(|(_, menu)| &menu.skin_id == skin_id)
            .map(|(index, _)| format!("menus[{index}].skin_id"))
            .collect(),
    }
}

pub fn references_to_asset(document: &RadialDocument, asset_id: &AssetId) -> ReferenceImpact {
    let needle = serde_json::to_value(MediaReference::Managed {
        asset_id: asset_id.clone(),
    })
    .expect("media reference serialization is infallible");
    let value = serde_json::to_value(document).expect("validated radial document serializes");
    let mut paths = Vec::new();
    collect_value_paths(&value, &needle, "$", &mut paths);
    ReferenceImpact { paths }
}

pub fn asset_can_be_deleted(document: &RadialDocument, asset_id: &AssetId) -> bool {
    references_to_asset(document, asset_id).paths.is_empty()
}

fn collect_value_paths(
    value: &serde_json::Value,
    needle: &serde_json::Value,
    path: &str,
    found: &mut Vec<String>,
) {
    if value == needle {
        found.push(path.to_string());
        return;
    }
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                collect_value_paths(value, needle, &format!("{path}.{key}"), found);
            }
        }
        serde_json::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_value_paths(value, needle, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::{AssetRecord, MediaKind, Override};
    use crate::radial::package::{IdRemap, PackageManifest};
    use base64::Engine;
    use std::collections::BTreeMap;
    use std::fs;
    fn fixture() -> (tempfile::TempDir, RadialStore) {
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(dir.path());
        (dir, RadialStore::new(&root).unwrap())
    }
    #[test]
    fn missing_empty_malformed_and_newer_are_distinct_and_retain_snapshot() {
        let (dir, store) = fixture();
        let before = store.snapshot().unwrap();
        assert!(matches!(
            store.reload(),
            Err(StoreError::Candidate(CandidateState::Missing))
        ));
        fs::write(dir.path().join(RADIAL_FILE), "  ").unwrap();
        assert!(matches!(
            store.reload(),
            Err(StoreError::Candidate(CandidateState::Empty))
        ));
        fs::write(dir.path().join(RADIAL_FILE), "{").unwrap();
        assert!(matches!(store.reload(), Err(StoreError::Malformed { .. })));
        fs::write(dir.path().join(RADIAL_FILE), r#"{"schema_version":999}"#).unwrap();
        assert!(matches!(
            store.reload(),
            Err(StoreError::UnsupportedNewerVersion { .. })
        ));
        assert!(Arc::ptr_eq(&before, &store.snapshot().unwrap()));
    }
    #[test]
    fn save_is_revision_checked_atomic_and_publishes_after_success() {
        let (_dir, store) = fixture();
        let before = store.snapshot().unwrap();
        let saved = store.save(before.revision, (*before).clone()).unwrap();
        assert_eq!(saved.revision.0, 2);
        assert_eq!(store.reload().unwrap().revision, saved.revision);
        assert!(matches!(
            store.save(ConfigRevision(1), (*saved).clone()),
            Err(StoreError::RevisionConflict { .. })
        ));
    }
    #[test]
    fn invalid_save_preserves_file_and_last_valid_snapshot() {
        let (dir, store) = fixture();
        let initial = store.snapshot().unwrap();
        let saved = store.save(initial.revision, (*initial).clone()).unwrap();
        let bytes = fs::read(dir.path().join(RADIAL_FILE)).unwrap();
        let mut bad = (*saved).clone();
        bad.default_menu_id = MenuId::new("missing");
        assert!(matches!(
            store.save(saved.revision, bad),
            Err(StoreError::Validation(_))
        ));
        assert_eq!(fs::read(dir.path().join(RADIAL_FILE)).unwrap(), bytes);
        assert_eq!(store.snapshot().unwrap().revision, saved.revision);
    }

    #[test]
    fn valid_external_change_publishes_once_with_owner_hook_before_swap() {
        let (dir, store) = fixture();
        let initial = store.snapshot().unwrap();
        let saved = store.save(initial.revision, (*initial).clone()).unwrap();
        let mut external = (*saved).clone();
        external.menus[0].name = "Externally edited".into();
        fs::write(
            dir.path().join(RADIAL_FILE),
            serde_json::to_vec_pretty(&external).unwrap(),
        )
        .unwrap();
        let hook_calls = std::cell::Cell::new(0);
        let outcome = store
            .reload_external_with(|candidate| {
                hook_calls.set(hook_calls.get() + 1);
                assert_eq!(candidate.revision.0, saved.revision.0 + 1);
                assert_eq!(store.snapshot().unwrap().menus[0].name, saved.menus[0].name);
            })
            .unwrap();
        let ExternalReloadOutcome::Published(published) = outcome else {
            panic!("external semantic change must publish")
        };
        assert_eq!(published.menus[0].name, "Externally edited");
        assert_eq!(hook_calls.get(), 1);
        assert_eq!(
            store
                .reload_external_with(|_| panic!("unchanged echo must not publish"))
                .unwrap(),
            ExternalReloadOutcome::Unchanged
        );
    }

    #[test]
    fn malformed_and_newer_external_bytes_are_retained_without_publishing() {
        let (dir, store) = fixture();
        let initial = store.snapshot().unwrap();
        let saved = store.save(initial.revision, (*initial).clone()).unwrap();
        for (bytes, newer) in [
            (b"{".as_slice(), false),
            (br#"{"schema_version":999}"#.as_slice(), true),
        ] {
            fs::write(dir.path().join(RADIAL_FILE), bytes).unwrap();
            let error = store
                .reload_external_with(|_| panic!("invalid bytes cannot reach owner hook"))
                .unwrap_err();
            assert_eq!(
                matches!(error, StoreError::UnsupportedNewerVersion { .. }),
                newer
            );
            assert_eq!(store.snapshot().unwrap().revision, saved.revision);
            assert_eq!(fs::read(dir.path().join(RADIAL_FILE)).unwrap(), bytes);
        }
    }

    #[test]
    fn self_save_identity_suppresses_filesystem_echo() {
        let (_dir, store) = fixture();
        let initial = store.snapshot().unwrap();
        let saved = store.save(initial.revision, (*initial).clone()).unwrap();
        assert_eq!(
            store
                .reload_external_with(|_| panic!("self-save echo must not publish"))
                .unwrap(),
            ExternalReloadOutcome::Unchanged
        );
        assert_eq!(store.snapshot().unwrap().revision, saved.revision);
    }
    #[test]
    fn impacts_are_explicit_before_deletion() {
        let d = RadialDocument::starter();
        assert_eq!(
            references_to_menu(&d, &d.default_menu_id).paths,
            vec!["default_menu_id"]
        );
        assert_eq!(
            references_to_skin(&d, &d.skins[0].id).paths,
            vec!["menus[0].skin_id"]
        );
    }

    #[test]
    fn at_path_rejects_an_invalid_initial_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let mut invalid = RadialDocument::starter();
        invalid.default_menu_id = MenuId::new("missing");
        assert!(matches!(
            RadialStore::at_path(directory.path().join(RADIAL_FILE), invalid),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn asset_publication_rejects_redirected_or_non_directory_roots() {
        let path = Path::new("radial_assets");
        assert!(validate_asset_root_kind(path, true, true, 0).is_err());
        assert!(validate_asset_root_kind(path, false, true, 0x400).is_err());
        assert!(validate_asset_root_kind(path, false, false, 0).is_err());
        assert!(validate_asset_root_kind(path, false, true, 0).is_ok());
    }

    #[test]
    fn legacy_preview_uses_transactional_create_new_store_boundary() {
        let (directory, store) = fixture();
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let inputs = [super::super::import::ImportInput {
            relative_path: "Skins/Legacy/ItemBack.png",
            bytes: &png,
            evidence: super::super::import::ImportEvidence::SyntheticFixture,
        }];
        let current = store.snapshot().unwrap();
        let preview = super::super::import::preview_radify(
            &inputs,
            "Legacy",
            &BTreeSet::from([current.default_menu_id.as_str().to_owned()]),
            &BTreeSet::from([current.skins[0].id.as_str().to_owned()]),
        );
        let saved = store
            .apply_legacy_import(
                &preview,
                current.revision,
                None,
                PackageApplyDecision::CreateNew,
            )
            .unwrap();
        assert_eq!(saved.menus.len(), current.menus.len() + 1);
        assert!(directory.path().join(RADIAL_ASSETS_DIRECTORY).is_dir());
        assert!(
            saved
                .assets
                .iter()
                .any(|asset| asset.content_sha256 == sha256_hex(&png))
        );
    }

    #[test]
    fn revision_overflow_is_reported_without_writing_or_publishing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(RADIAL_FILE);
        let mut initial = RadialDocument::starter();
        initial.revision = ConfigRevision(u64::MAX);
        let store = RadialStore::at_path(&path, initial.clone()).unwrap();
        assert!(matches!(
            store.save(initial.revision, initial.clone()),
            Err(StoreError::RevisionOverflow {
                revision: ConfigRevision(u64::MAX)
            })
        ));
        assert!(!path.exists());
        assert_eq!(store.snapshot().unwrap().revision, ConfigRevision(u64::MAX));
    }

    #[test]
    fn v1_reload_migrates_only_in_memory_and_next_save_writes_current_schema() {
        let (directory, store) = fixture();
        let path = directory.path().join(RADIAL_FILE);
        let mut value = serde_json::to_value(RadialDocument::starter()).unwrap();
        value["schema_version"] = 1.into();
        let document = value.as_object_mut().unwrap();
        document.remove("user_style_defaults");
        document.remove("media_search_roots");
        document.remove("assets");
        for menu in document["menus"].as_array_mut().unwrap() {
            menu.as_object_mut().unwrap().remove("style");
            for ring in menu["rings"].as_array_mut().unwrap() {
                ring.as_object_mut().unwrap().remove("style");
                for cell in ring["cells"].as_array_mut().unwrap() {
                    let cell = cell.as_object_mut().unwrap();
                    cell.remove("tooltip");
                    cell.remove("style");
                    cell.remove("shortcuts");
                    cell.remove("hotstrings");
                }
            }
        }
        for skin in value["skins"].as_array_mut().unwrap() {
            skin.as_object_mut().unwrap().remove("style");
            skin["scale"] = 1.0.into();
            skin["enable_glow"] = serde_json::json!({ "Value": false });
            skin["center_image"] = serde_json::json!("Clear");
        }
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        fs::write(&path, &bytes).unwrap();
        let migrated = store.reload().unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(fs::read(&path).unwrap(), bytes, "reload is read-only");
        let saved = store.save(migrated.revision, (*migrated).clone()).unwrap();
        assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
        let saved_value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(saved_value["schema_version"], CURRENT_SCHEMA_VERSION);
        assert!(saved_value["skins"][0].get("scale").is_none());
    }

    fn imported_plan_with_asset(target: &RadialDocument) -> ImportPlan {
        let mut document = RadialDocument::starter();
        document.default_menu_id = MenuId::new("imported-menu");
        document.menus[0].id = document.default_menu_id.clone();
        document.menus[0].skin_id = SkinId::new("imported-skin");
        document.skins[0].id = SkinId::new("imported-skin");
        document.menus[0].rings[0].id = super::super::model::RingId::new("imported-ring");
        for (index, cell) in document.menus[0].rings[0].cells.iter_mut().enumerate() {
            cell.id = super::super::model::CellId::new(format!("imported-cell-{index}"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let digest = sha256_hex(&bytes);
        let asset_id = AssetId::new("imported-asset");
        document.skins[0].style.values.images.center_image =
            Override::Value(MediaReference::Managed {
                asset_id: asset_id.clone(),
            });
        document.assets.push(AssetRecord {
            id: asset_id,
            kind: MediaKind::Image,
            relative_path: "legacy.png".into(),
            content_sha256: digest.clone(),
            byte_len: bytes.len() as u64,
        });
        assert!(
            !target
                .menus
                .iter()
                .any(|menu| menu.id == document.default_menu_id)
        );
        ImportPlan {
            manifest: PackageManifest {
                package_version: 1,
                document_path: "radial.json".into(),
                root_menu_ids: vec![document.default_menu_id.clone()],
                files: vec![],
                notices: vec![],
            },
            document,
            assets: std::collections::BTreeMap::from([(format!("assets/{digest}.png"), bytes)]),
            remap: IdRemap::default(),
        }
    }

    struct CancelAt(ApplyStep);
    impl PackageApplyHook for CancelAt {
        fn checkpoint(&mut self, step: ApplyStep) -> bool {
            step != self.0
        }
    }

    #[test]
    fn package_apply_failure_rolls_back_only_created_assets_and_keeps_snapshot_json() {
        for step in [
            ApplyStep::BeforeAssets,
            ApplyStep::AfterAsset,
            ApplyStep::BeforeDocument,
        ] {
            let (directory, store) = fixture();
            let before = store.snapshot().unwrap();
            let request = PackageApplyRequest {
                expected_revision: before.revision,
                expected_disk_sha256: Some(sha256_hex(&[])),
                expected_asset_sha256: Default::default(),
                decision: PackageApplyDecision::CreateNew,
                plan: imported_plan_with_asset(&before),
            };
            assert!(matches!(
                store.apply_package_with_hook(request, &mut CancelAt(step)),
                Err(StoreError::ApplyCancelled(actual)) if actual == step
            ));
            assert!(!directory.path().join(RADIAL_FILE).exists());
            assert_eq!(store.snapshot().unwrap().revision, before.revision);
            let asset_root = directory.path().join(RADIAL_ASSETS_DIRECTORY);
            assert!(
                !asset_root.exists() || std::fs::read_dir(asset_root).unwrap().next().is_none(),
                "transaction-owned asset was removed"
            );
        }
    }

    #[test]
    fn package_apply_publishes_assets_then_json_and_usage_blocks_deletion() {
        let (directory, store) = fixture();
        let before = store.snapshot().unwrap();
        let request = PackageApplyRequest {
            expected_revision: before.revision,
            expected_disk_sha256: Some(sha256_hex(&[])),
            expected_asset_sha256: Default::default(),
            decision: PackageApplyDecision::CreateNew,
            plan: imported_plan_with_asset(&before),
        };
        let saved = store.apply_package(request).unwrap();
        assert_eq!(saved.revision.0, before.revision.0 + 1);
        let asset = saved
            .assets
            .iter()
            .find(|asset| asset.id.as_str() == "imported-asset")
            .unwrap();
        assert!(
            directory
                .path()
                .join(RADIAL_ASSETS_DIRECTORY)
                .join(&asset.relative_path)
                .exists()
        );
        assert!(!asset_can_be_deleted(&saved, &asset.id));
        assert!(!references_to_asset(&saved, &asset.id).paths.is_empty());
        assert_eq!(store.reload().unwrap().revision, saved.revision);
    }

    #[test]
    fn cancelled_apply_never_deletes_a_preexisting_content_addressed_asset() {
        let (directory, store) = fixture();
        let before = store.snapshot().unwrap();
        let plan = imported_plan_with_asset(&before);
        let asset = &plan.document.assets[0];
        let digest = asset.content_sha256.clone();
        let bytes = plan.assets.values().next().unwrap();
        let path = directory
            .path()
            .join(RADIAL_ASSETS_DIRECTORY)
            .join(format!("{digest}.png"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let request = PackageApplyRequest {
            expected_revision: before.revision,
            expected_disk_sha256: Some(sha256_hex(&[])),
            expected_asset_sha256: BTreeMap::from([(path.clone(), digest.clone())]),
            decision: PackageApplyDecision::CreateNew,
            plan,
        };
        assert!(matches!(
            store.apply_package_with_hook(request, &mut CancelAt(ApplyStep::BeforeDocument)),
            Err(StoreError::ApplyCancelled(ApplyStep::BeforeDocument))
        ));
        assert_eq!(sha256_hex(&std::fs::read(path).unwrap()), digest);
    }

    #[test]
    fn package_apply_rechecks_disk_and_replace_backup_receipt() {
        let (directory, store) = fixture();
        let before = store.snapshot().unwrap();
        std::fs::write(directory.path().join(RADIAL_FILE), b"changed").unwrap();
        let request = PackageApplyRequest {
            expected_revision: before.revision,
            expected_disk_sha256: Some(sha256_hex(&[])),
            expected_asset_sha256: Default::default(),
            decision: PackageApplyDecision::CreateNew,
            plan: imported_plan_with_asset(&before),
        };
        assert!(matches!(
            store.apply_package(request),
            Err(StoreError::DiskChanged)
        ));

        let disk = std::fs::read(directory.path().join(RADIAL_FILE)).unwrap();
        let backup = directory.path().join("backup.json");
        std::fs::write(&backup, b"wrong").unwrap();
        let request = PackageApplyRequest {
            expected_revision: before.revision,
            expected_disk_sha256: Some(sha256_hex(&disk)),
            expected_asset_sha256: Default::default(),
            decision: PackageApplyDecision::Replace {
                backup: BackupReceipt {
                    path: backup,
                    source_sha256: sha256_hex(&disk),
                },
            },
            plan: imported_plan_with_asset(&before),
        };
        assert!(matches!(
            store.apply_package(request),
            Err(StoreError::InvalidBackup)
        ));
        assert_eq!(
            std::fs::read(directory.path().join(RADIAL_FILE)).unwrap(),
            disk
        );
        assert_eq!(store.snapshot().unwrap().revision, before.revision);

        let backup = directory.path().join("verified-backup.json");
        std::fs::write(&backup, &disk).unwrap();
        let request = PackageApplyRequest {
            expected_revision: before.revision,
            expected_disk_sha256: Some(sha256_hex(&disk)),
            expected_asset_sha256: Default::default(),
            decision: PackageApplyDecision::Replace {
                backup: BackupReceipt {
                    path: backup,
                    source_sha256: sha256_hex(&disk),
                },
            },
            plan: imported_plan_with_asset(&before),
        };
        let replaced = store.apply_package(request).unwrap();
        assert_eq!(replaced.default_menu_id.as_str(), "imported-menu");
        assert_eq!(replaced.revision.0, before.revision.0 + 1);
    }
}
