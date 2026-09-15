use super::authoring::{AssetMutations, AuthoringSnapshot, DiskSha256, ManagedAssetAddition};
use super::migration::{DocumentDecodeError, decode_document};
use super::model::{
    AssetId, CURRENT_SCHEMA_VERSION, ConfigRevision, MediaReference, MenuId,
    RADIAL_ASSETS_DIRECTORY, RADIAL_FILE, RadialDocument, SkinId,
};
use super::package::{
    ImportPlan, PackageError, encode_mlradial, plan_export, plan_skin_export, sha256_hex,
};
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
    AssetReferenced {
        asset: AssetId,
        paths: Vec<String>,
    },
    AssetMutationMismatch {
        asset: AssetId,
        reason: String,
    },
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
            Self::AssetReferenced { asset, paths } => write!(
                f,
                "managed asset {asset} is still referenced by {}",
                paths.join(", ")
            ),
            Self::AssetMutationMismatch { asset, reason } => {
                write!(f, "managed asset mutation for {asset} is invalid: {reason}")
            }
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
pub struct AuthoringCommitResult {
    pub snapshot: AuthoringSnapshot,
    /// Inverse managed-asset transaction retained by the editor so Cancel can
    /// revert its last successful Apply through the same checked store path.
    pub rollback_assets: AssetMutations,
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

    pub fn authoring_snapshot(&self) -> Result<AuthoringSnapshot, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let document = self
            .published
            .read()
            .map_err(|_| StoreError::LockPoisoned)?
            .clone();
        let bytes = match read_bytes(&self.path)? {
            LoadState::Loaded(bytes) => bytes,
            LoadState::Missing | LoadState::Empty => Vec::new(),
        };
        if !bytes.is_empty() {
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
            let mut comparable = decoded.document;
            comparable.schema_version = document.schema_version;
            comparable.revision = document.revision;
            if comparable != *document {
                return Err(StoreError::DiskChanged);
            }
        }
        Ok(AuthoringSnapshot::new(document, sha256_hex(&bytes)))
    }

    /// Main-owner transaction used by authoring Apply/Save/Cancel. Managed
    /// assets and the validated document publish atomically from the runtime's
    /// perspective. New files are removed and staged deletions restored if the
    /// JSON commit fails.
    pub fn commit_authoring(
        &self,
        expected_revision: ConfigRevision,
        expected_disk_sha256: &DiskSha256,
        mut candidate: RadialDocument,
        assets: AssetMutations,
    ) -> Result<AuthoringCommitResult, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let current = self
            .published
            .read()
            .map_err(|_| StoreError::LockPoisoned)?
            .clone();
        if current.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }
        let disk_bytes = match read_bytes(&self.path)? {
            LoadState::Loaded(bytes) => bytes,
            LoadState::Missing | LoadState::Empty => Vec::new(),
        };
        if sha256_hex(&disk_bytes) != expected_disk_sha256.0 {
            return Err(StoreError::DiskChanged);
        }

        candidate.schema_version = CURRENT_SCHEMA_VERSION;
        candidate.revision = ConfigRevision(current.revision.0.checked_add(1).ok_or(
            StoreError::RevisionOverflow {
                revision: current.revision,
            },
        )?);

        let mut seen_additions = BTreeSet::new();
        for addition in &assets.additions {
            let id = addition.record.id.clone();
            if !seen_additions.insert(id.clone()) {
                return Err(StoreError::AssetMutationMismatch {
                    asset: id,
                    reason: "duplicate addition".into(),
                });
            }
            let candidate_record = candidate
                .assets
                .iter()
                .find(|asset| asset.id == id)
                .ok_or_else(|| StoreError::AssetMutationMismatch {
                    asset: id.clone(),
                    reason: "candidate does not contain the added record".into(),
                })?;
            if candidate_record != &addition.record
                || addition.record.byte_len != addition.bytes.len() as u64
                || addition.record.content_sha256 != sha256_hex(&addition.bytes)
            {
                return Err(StoreError::AssetMutationMismatch {
                    asset: id,
                    reason: "record, byte length, or SHA-256 does not match supplied bytes".into(),
                });
            }
            if current
                .assets
                .iter()
                .find(|asset| asset.id == id)
                .is_some_and(|asset| asset != &addition.record)
            {
                return Err(StoreError::AssetMutationMismatch {
                    asset: id,
                    reason: "an existing stable asset ID cannot be rebound to different content"
                        .into(),
                });
            }
            super::assets::validate_packaged_media(&addition.bytes, addition.record.kind).map_err(
                |reason| StoreError::AssetMutationMismatch {
                    asset: addition.record.id.clone(),
                    reason: reason.to_string(),
                },
            )?;
        }

        let mut seen_deletions = BTreeSet::new();
        for id in &assets.deletions {
            if !seen_deletions.insert(id.clone()) {
                return Err(StoreError::AssetMutationMismatch {
                    asset: id.clone(),
                    reason: "duplicate deletion".into(),
                });
            }
            let impact = references_to_asset(&candidate, id);
            if !impact.paths.is_empty() || candidate.assets.iter().any(|asset| &asset.id == id) {
                return Err(StoreError::AssetReferenced {
                    asset: id.clone(),
                    paths: impact.paths,
                });
            }
            if let Some(record) = current.assets.iter().find(|asset| &asset.id == id)
                && candidate
                    .assets
                    .iter()
                    .any(|asset| asset.relative_path == record.relative_path)
            {
                return Err(StoreError::AssetReferenced {
                    asset: id.clone(),
                    paths: vec![format!(
                        "managed path {} remains owned by another asset record",
                        record.relative_path
                    )],
                });
            }
        }
        validate(&candidate)?;

        // Acquire every fallible publication lock before touching disk. Once
        // JSON commits, publishing the matching in-memory snapshot and identity
        // is therefore infallible under these retained guards.
        let mut published_guard = self
            .published
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut disk_identity_guard = self
            .disk_identity
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;

        let asset_root = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(RADIAL_ASSETS_DIRECTORY);
        reject_reparse_asset_root(&asset_root)?;
        let asset_root_existed = asset_root.exists();
        if !assets.additions.is_empty() {
            std::fs::create_dir_all(&asset_root).map_err(|source| StoreError::ApplyIo {
                path: asset_root.clone(),
                source,
            })?;
        }

        let mut rollback_assets = AssetMutations::default();
        let mut created = Vec::new();
        let mut staged_deletes: Vec<(PathBuf, PathBuf)> = Vec::new();
        let result = (|| {
            for addition in &assets.additions {
                let path = asset_root.join(&addition.record.relative_path);
                if path.exists() {
                    let existing = std::fs::read(&path).map_err(|source| StoreError::ApplyIo {
                        path: path.clone(),
                        source,
                    })?;
                    if sha256_hex(&existing) != addition.record.content_sha256 {
                        return Err(StoreError::DiskChanged);
                    }
                } else {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .map_err(|source| StoreError::ApplyIo {
                            path: path.clone(),
                            source,
                        })?;
                    if let Err(source) = file
                        .write_all(&addition.bytes)
                        .and_then(|_| file.sync_all())
                    {
                        let _ = std::fs::remove_file(&path);
                        return Err(StoreError::ApplyIo { path, source });
                    }
                    created.push(path);
                }
                if !current
                    .assets
                    .iter()
                    .any(|asset| asset.id == addition.record.id)
                {
                    rollback_assets.deletions.push(addition.record.id.clone());
                }
            }

            for (delete_index, id) in assets.deletions.iter().enumerate() {
                let Some(record) = current.assets.iter().find(|asset| &asset.id == id) else {
                    continue;
                };
                let path = asset_root.join(&record.relative_path);
                if !path.exists() {
                    continue;
                }
                let bytes = std::fs::read(&path).map_err(|source| StoreError::ApplyIo {
                    path: path.clone(),
                    source,
                })?;
                if sha256_hex(&bytes) != record.content_sha256 {
                    return Err(StoreError::DiskChanged);
                }
                rollback_assets.additions.push(ManagedAssetAddition {
                    record: record.clone(),
                    bytes: Arc::from(bytes),
                });
                let staged = path.with_extension(format!(
                    "authoring-delete-{}-{}",
                    candidate.revision.0, delete_index
                ));
                std::fs::rename(&path, &staged).map_err(|source| StoreError::ApplyIo {
                    path: path.clone(),
                    source,
                })?;
                staged_deletes.push((path, staged));
            }

            save_json_atomic(&self.path, &candidate)?;
            Ok::<(), StoreError>(())
        })();

        if let Err(error) = result {
            for path in created.iter().rev() {
                let _ = std::fs::remove_file(path);
            }
            for (path, staged) in staged_deletes.iter().rev() {
                let _ = std::fs::rename(staged, path);
            }
            if !asset_root_existed {
                let _ = std::fs::remove_dir(&asset_root);
            }
            return Err(error);
        }
        for (_, staged) in &staged_deletes {
            let _ = std::fs::remove_file(staged);
        }

        let saved_bytes = serde_json::to_vec_pretty(&candidate).map_err(|source| {
            PersistenceError::SerializeJson {
                path: self.path.clone(),
                source,
            }
        })?;
        let disk_sha = sha256_hex(&saved_bytes);
        let candidate = Arc::new(candidate);
        *published_guard = Arc::clone(&candidate);
        *disk_identity_guard = Some(disk_sha.clone());
        Ok(AuthoringCommitResult {
            snapshot: AuthoringSnapshot::new(candidate, disk_sha),
            rollback_assets,
        })
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

    /// Build a dependency-complete portable package from persisted, verified
    /// managed bytes. GUI code never receives or derives the store root.
    pub fn export_package(
        &self,
        roots: &[MenuId],
        expected_revision: ConfigRevision,
        expected_disk_sha256: &str,
    ) -> Result<Vec<u8>, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let document = self
            .published
            .read()
            .map_err(|_| StoreError::LockPoisoned)?
            .clone();
        if document.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                expected: expected_revision,
                actual: document.revision,
            });
        }
        let disk = match read_bytes(&self.path)? {
            LoadState::Loaded(bytes) => bytes,
            LoadState::Missing | LoadState::Empty => Vec::new(),
        };
        if sha256_hex(&disk) != expected_disk_sha256 {
            return Err(StoreError::DiskChanged);
        }
        let asset_root = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(RADIAL_ASSETS_DIRECTORY);
        reject_reparse_asset_root(&asset_root)?;
        let canonical_asset_root = std::fs::canonicalize(&asset_root).ok();
        let mut bytes = std::collections::BTreeMap::new();
        loop {
            let asset_id = match plan_export(&document, roots, &bytes, Vec::new()) {
                Ok(plan) => return encode_mlradial(&plan).map_err(Into::into),
                Err(PackageError::MissingAsset(asset_id)) => asset_id,
                Err(error) => return Err(error.into()),
            };
            let asset = document
                .assets
                .iter()
                .find(|asset| asset.id == asset_id)
                .ok_or_else(|| StoreError::Package(PackageError::MissingAsset(asset_id)))?;
            let path = asset_root.join(&asset.relative_path);
            let metadata =
                std::fs::symlink_metadata(&path).map_err(|source| StoreError::ApplyIo {
                    path: path.clone(),
                    source,
                })?;
            #[cfg(windows)]
            use std::os::windows::fs::MetadataExt;
            #[cfg(windows)]
            let reparse = metadata.file_attributes() & 0x400 != 0;
            #[cfg(not(windows))]
            let reparse = false;
            let escapes_root = canonical_asset_root.as_ref().is_some_and(|root| {
                std::fs::canonicalize(&path)
                    .map(|candidate| !candidate.starts_with(root))
                    .unwrap_or(true)
            });
            if reparse || metadata.file_type().is_symlink() || !metadata.is_file() || escapes_root {
                return Err(StoreError::ApplyIo {
                    path,
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "managed export asset must be a regular non-link file",
                    ),
                });
            }
            let content = std::fs::read(&path).map_err(|source| StoreError::ApplyIo {
                path: path.clone(),
                source,
            })?;
            if content.len() as u64 != asset.byte_len
                || sha256_hex(&content) != asset.content_sha256
            {
                return Err(StoreError::Package(PackageError::ChecksumMismatch(
                    asset.relative_path.clone(),
                )));
            }
            bytes.insert(asset.id.clone(), content);
        }
    }

    pub fn export_skin_bundle(
        &self,
        skin_id: &super::model::SkinId,
        expected_revision: ConfigRevision,
        expected_disk_sha256: &str,
    ) -> Result<Vec<u8>, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let document = self
            .published
            .read()
            .map_err(|_| StoreError::LockPoisoned)?
            .clone();
        if document.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                expected: expected_revision,
                actual: document.revision,
            });
        }
        let disk = match read_bytes(&self.path)? {
            LoadState::Loaded(bytes) => bytes,
            LoadState::Missing | LoadState::Empty => Vec::new(),
        };
        if sha256_hex(&disk) != expected_disk_sha256 {
            return Err(StoreError::DiskChanged);
        }
        let asset_root = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(RADIAL_ASSETS_DIRECTORY);
        reject_reparse_asset_root(&asset_root)?;
        let canonical_asset_root = std::fs::canonicalize(&asset_root).ok();
        let mut bytes = std::collections::BTreeMap::new();
        loop {
            let asset_id = match plan_skin_export(&document, skin_id, &bytes, Vec::new()) {
                Ok(plan) => return encode_mlradial(&plan).map_err(Into::into),
                Err(PackageError::MissingAsset(asset_id)) => asset_id,
                Err(error) => return Err(error.into()),
            };
            let asset = document
                .assets
                .iter()
                .find(|asset| asset.id == asset_id)
                .ok_or_else(|| StoreError::Package(PackageError::MissingAsset(asset_id)))?;
            let path = asset_root.join(&asset.relative_path);
            let metadata =
                std::fs::symlink_metadata(&path).map_err(|source| StoreError::ApplyIo {
                    path: path.clone(),
                    source,
                })?;
            #[cfg(windows)]
            use std::os::windows::fs::MetadataExt;
            #[cfg(windows)]
            let reparse = metadata.file_attributes() & 0x400 != 0;
            #[cfg(not(windows))]
            let reparse = false;
            let escapes_root = canonical_asset_root.as_ref().is_some_and(|root| {
                std::fs::canonicalize(&path)
                    .map(|candidate| !candidate.starts_with(root))
                    .unwrap_or(true)
            });
            if reparse || metadata.file_type().is_symlink() || !metadata.is_file() || escapes_root {
                return Err(StoreError::ApplyIo {
                    path,
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "managed export asset must be a regular non-link file",
                    ),
                });
            }
            let content = std::fs::read(&path).map_err(|source| StoreError::ApplyIo {
                path: path.clone(),
                source,
            })?;
            if content.len() as u64 != asset.byte_len
                || sha256_hex(&content) != asset.content_sha256
            {
                return Err(StoreError::Package(PackageError::ChecksumMismatch(
                    asset.relative_path.clone(),
                )));
            }
            bytes.insert(asset.id.clone(), content);
        }
    }

    /// Create an exact atomic backup owned by main, verify its receipt, then
    /// enter the existing transactional replacement boundary.
    pub fn replace_package_with_backup(
        &self,
        plan: ImportPlan,
        expected_revision: ConfigRevision,
        expected_disk_sha256: String,
        backup_path: PathBuf,
    ) -> Result<Arc<RadialDocument>, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let actual_revision = self
            .published
            .read()
            .map_err(|_| StoreError::LockPoisoned)?
            .revision;
        if actual_revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        let source_parent =
            std::fs::canonicalize(self.path.parent().unwrap_or_else(|| Path::new("."))).ok();
        let backup_parent =
            std::fs::canonicalize(backup_path.parent().unwrap_or_else(|| Path::new("."))).ok();
        let asset_root = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(RADIAL_ASSETS_DIRECTORY);
        let backup_overwrites_source = source_parent
            .clone()
            .zip(backup_parent.clone())
            .is_some_and(|(source, backup)| {
                source == backup && self.path.file_name() == backup_path.file_name()
            });
        let backup_enters_asset_store = std::fs::canonicalize(asset_root)
            .ok()
            .zip(backup_parent)
            .is_some_and(|(assets, backup)| backup.starts_with(assets));
        if backup_overwrites_source || backup_enters_asset_store {
            return Err(StoreError::InvalidBackup);
        }
        let disk = match read_bytes(&self.path)? {
            LoadState::Loaded(bytes) => bytes,
            LoadState::Missing | LoadState::Empty => Vec::new(),
        };
        if sha256_hex(&disk) != expected_disk_sha256 {
            return Err(StoreError::DiskChanged);
        }
        crate::common::atomic_file::save_atomic(&backup_path, &disk).map_err(|source| {
            StoreError::ApplyIo {
                path: backup_path.clone(),
                source: std::io::Error::other(source.to_string()),
            }
        })?;
        let backup_bytes = std::fs::read(&backup_path).map_err(|source| StoreError::ApplyIo {
            path: backup_path.clone(),
            source,
        })?;
        if sha256_hex(&backup_bytes) != expected_disk_sha256 {
            return Err(StoreError::InvalidBackup);
        }
        self.apply_package_locked(
            PackageApplyRequest {
                expected_revision,
                expected_disk_sha256: Some(expected_disk_sha256.clone()),
                expected_asset_sha256: Default::default(),
                decision: PackageApplyDecision::Replace {
                    backup: BackupReceipt {
                        path: backup_path,
                        source_sha256: expected_disk_sha256,
                    },
                },
                plan,
            },
            &mut (),
        )
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
        self.apply_package_locked(request, hook)
    }

    fn apply_package_locked(
        &self,
        request: PackageApplyRequest,
        hook: &mut impl PackageApplyHook,
    ) -> Result<Arc<RadialDocument>, StoreError> {
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
            (0..d.menus.len())
                .map(|index| format!("menus[{index}].skin_id"))
                .collect::<Vec<_>>()
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
        // This fixture represents a self-contained one-menu package, not the
        // evolving user-facing starter graph.
        document.menus.truncate(1);
        document.menus[0].rings[0].cells.truncate(1);
        document.menus[0].rings[0].cells[0].content = super::super::model::CellContent::Spacer;
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
                payload: super::super::package::PackagePayloadKind::MenuGraph,
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
    fn persisted_asset_export_roundtrips_and_reports_missing_or_tampered_bytes() {
        let (directory, store) = fixture();
        let before = store.snapshot().unwrap();
        let saved = store
            .apply_package(PackageApplyRequest {
                expected_revision: before.revision,
                expected_disk_sha256: Some(sha256_hex(&[])),
                expected_asset_sha256: Default::default(),
                decision: PackageApplyDecision::CreateNew,
                plan: imported_plan_with_asset(&before),
            })
            .unwrap();
        let root = MenuId::new("imported-menu");
        let export_snapshot = store.authoring_snapshot().unwrap();
        let package = store
            .export_package(
                std::slice::from_ref(&root),
                export_snapshot.revision,
                &export_snapshot.disk_sha256.0,
            )
            .unwrap();
        let files = super::super::package::decode_mlradial(&package).unwrap();
        let imported =
            super::super::package::plan_import(files, &RadialDocument::starter()).unwrap();
        assert_eq!(imported.document.assets.len(), 1);
        assert_eq!(imported.assets.len(), 1);
        assert!(matches!(
            store.export_package(
                std::slice::from_ref(&root),
                ConfigRevision(export_snapshot.revision.0 + 1),
                &export_snapshot.disk_sha256.0,
            ),
            Err(StoreError::RevisionConflict { .. })
        ));

        let asset_path = directory.path().join(RADIAL_ASSETS_DIRECTORY).join(
            &saved
                .assets
                .iter()
                .find(|asset| asset.id.as_str() == "imported-asset")
                .unwrap()
                .relative_path,
        );
        std::fs::write(&asset_path, b"tampered").unwrap();
        assert!(matches!(
            store.export_package(
                std::slice::from_ref(&root),
                export_snapshot.revision,
                &export_snapshot.disk_sha256.0,
            ),
            Err(StoreError::Package(PackageError::ChecksumMismatch(_)))
        ));
        std::fs::remove_file(&asset_path).unwrap();
        assert!(matches!(
            store.export_package(
                &[root],
                export_snapshot.revision,
                &export_snapshot.disk_sha256.0,
            ),
            Err(StoreError::ApplyIo { .. })
        ));
    }

    #[test]
    fn replace_service_requires_current_revision_and_verified_backup_and_rolls_back_failure() {
        let (directory, store) = fixture();
        let initial = store.snapshot().unwrap();
        store.save(initial.revision, (*initial).clone()).unwrap();
        let before = store.authoring_snapshot().unwrap();
        let stale_backup = directory.path().join("stale-backup.json");
        assert!(matches!(
            store.replace_package_with_backup(
                imported_plan_with_asset(&before.document),
                ConfigRevision(before.revision.0.saturating_sub(1)),
                before.disk_sha256.0.clone(),
                stale_backup.clone(),
            ),
            Err(StoreError::RevisionConflict { .. })
        ));
        assert!(!stale_backup.exists());

        let old_disk = std::fs::read(directory.path().join(RADIAL_FILE)).unwrap();
        let mut invalid = imported_plan_with_asset(&before.document);
        invalid.document.default_menu_id = MenuId::new("missing-default");
        let rollback_backup = directory.path().join("rollback-backup.json");
        assert!(
            store
                .replace_package_with_backup(
                    invalid,
                    before.revision,
                    before.disk_sha256.0.clone(),
                    rollback_backup.clone(),
                )
                .is_err()
        );
        assert_eq!(
            std::fs::read(directory.path().join(RADIAL_FILE)).unwrap(),
            old_disk
        );
        assert_eq!(store.snapshot().unwrap().revision, before.revision);
        assert_eq!(std::fs::read(rollback_backup).unwrap(), old_disk);

        let backup = directory.path().join("verified-backup.json");
        let replaced = store
            .replace_package_with_backup(
                imported_plan_with_asset(&before.document),
                before.revision,
                before.disk_sha256.0,
                backup.clone(),
            )
            .unwrap();
        assert_eq!(replaced.default_menu_id.as_str(), "imported-menu");
        assert_eq!(std::fs::read(backup).unwrap(), old_disk);
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

    fn authoring_png_addition(id: &str) -> ManagedAssetAddition {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let digest = sha256_hex(&bytes);
        ManagedAssetAddition {
            record: AssetRecord {
                id: AssetId::new(id),
                kind: MediaKind::Image,
                relative_path: format!("{digest}.png"),
                content_sha256: digest,
                byte_len: bytes.len() as u64,
            },
            bytes: Arc::from(bytes),
        }
    }

    #[test]
    fn authoring_revision_or_sha_conflict_creates_no_partial_asset() {
        let (directory, store) = fixture();
        let snapshot = store.authoring_snapshot().unwrap();
        let addition = authoring_png_addition("editor-asset");
        let path = directory
            .path()
            .join(RADIAL_ASSETS_DIRECTORY)
            .join(&addition.record.relative_path);
        let mut candidate = (*snapshot.document).clone();
        candidate.assets.push(addition.record.clone());
        assert!(matches!(
            store.commit_authoring(
                ConfigRevision(snapshot.revision.0 + 1),
                &snapshot.disk_sha256,
                candidate.clone(),
                AssetMutations {
                    additions: vec![addition.clone()],
                    deletions: vec![]
                },
            ),
            Err(StoreError::RevisionConflict { .. })
        ));
        assert!(!path.exists());
        assert!(matches!(
            store.commit_authoring(
                snapshot.revision,
                &DiskSha256("wrong".into()),
                candidate,
                AssetMutations {
                    additions: vec![addition],
                    deletions: vec![]
                },
            ),
            Err(StoreError::DiskChanged)
        ));
        assert!(!path.exists());
        assert!(!directory.path().join(RADIAL_FILE).exists());
    }

    #[test]
    fn authoring_snapshot_never_pairs_retained_runtime_state_with_malformed_disk_bytes() {
        let (directory, store) = fixture();
        std::fs::write(directory.path().join(RADIAL_FILE), b"{").unwrap();
        assert!(matches!(
            store.authoring_snapshot(),
            Err(StoreError::Malformed { .. })
        ));
        assert_eq!(store.snapshot().unwrap().revision, ConfigRevision(1));
    }

    #[test]
    fn authoring_asset_and_document_publish_together_and_return_inverse() {
        let (directory, store) = fixture();
        let snapshot = store.authoring_snapshot().unwrap();
        let addition = authoring_png_addition("editor-asset");
        let path = directory
            .path()
            .join(RADIAL_ASSETS_DIRECTORY)
            .join(&addition.record.relative_path);
        let mut candidate = (*snapshot.document).clone();
        candidate.menus[0].name = "Authored".into();
        candidate.assets.push(addition.record.clone());
        let published = store
            .commit_authoring(
                snapshot.revision,
                &snapshot.disk_sha256,
                candidate,
                AssetMutations {
                    additions: vec![addition.clone()],
                    deletions: vec![],
                },
            )
            .unwrap();
        assert_eq!(published.snapshot.document.menus[0].name, "Authored");
        assert!(path.exists());
        assert_eq!(
            published.rollback_assets.deletions,
            vec![addition.record.id.clone()]
        );

        let mut reverted = (*published.snapshot.document).clone();
        reverted
            .assets
            .retain(|asset| asset.id != addition.record.id);
        let reverted = store
            .commit_authoring(
                published.snapshot.revision,
                &published.snapshot.disk_sha256,
                reverted,
                published.rollback_assets,
            )
            .unwrap();
        assert!(!path.exists());
        assert!(reverted.snapshot.document.assets.is_empty());
        assert_eq!(store.reload().unwrap().revision, reverted.snapshot.revision);
    }

    #[test]
    fn authoring_delete_is_guarded_by_candidate_references() {
        let (_directory, store) = fixture();
        let snapshot = store.authoring_snapshot().unwrap();
        let addition = authoring_png_addition("editor-asset");
        let mut candidate = (*snapshot.document).clone();
        candidate.skins[0].style.values.images.center_image =
            Override::Value(MediaReference::Managed {
                asset_id: addition.record.id.clone(),
            });
        assert!(matches!(
            store.commit_authoring(
                snapshot.revision,
                &snapshot.disk_sha256,
                candidate,
                AssetMutations {
                    additions: vec![],
                    deletions: vec![addition.record.id]
                },
            ),
            Err(StoreError::AssetReferenced { .. })
        ));
    }
}
