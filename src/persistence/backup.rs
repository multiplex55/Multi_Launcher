use super::{BackupPolicy, PersistenceCatalog, PersistentStoreId, StoreDescriptor, StoreKind};
use crate::common::persistence::save_json_atomic;
use crate::platform::app_data::AppDataRoot;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs::{self, Metadata};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const BACKUP_DIRECTORY: &str = "backups";
const STAGING_PREFIX: &str = ".multi-launcher-staging-";
const MANIFEST_FILE: &str = "manifest.json";
const PRODUCT: &str = "Multi Launcher";
pub const BACKUP_FORMAT_VERSION: u32 = 1;
pub const SNAPSHOT_RETENTION_LIMIT: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotEntry {
    pub store_id: String,
    pub source_path: PathBuf,
    pub snapshot_path: Option<PathBuf>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub product: String,
    pub format_version: u32,
    pub app_version: String,
    pub snapshot_id: String,
    pub created_unix_millis: u128,
    pub source_root: PathBuf,
    /// Each file is copied from one open to one staging write. Stores may change
    /// between entries, so this is deliberately not a cross-store transaction.
    pub consistency: String,
    pub status: SnapshotStatus,
    pub included: Vec<SnapshotEntry>,
    pub missing: Vec<SnapshotEntry>,
    pub skipped: Vec<SnapshotEntry>,
    pub failed: Vec<SnapshotEntry>,
    pub external: Vec<SnapshotEntry>,
}

#[derive(Clone, Debug)]
pub struct SnapshotResult {
    pub path: PathBuf,
    pub manifest: SnapshotManifest,
    /// A finalized snapshot remains a success even if best-effort retention
    /// cannot remove an older one. Surface this warning without making the
    /// caller guess whether the new snapshot exists.
    pub retention_warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRecord {
    pub path: PathBuf,
    pub manifest: SnapshotManifest,
}

pub trait SnapshotClock {
    fn created_unix_millis(&self) -> u128;
    fn base_id(&self) -> String;
}

#[derive(Default)]
pub struct SystemSnapshotClock;

impl SnapshotClock for SystemSnapshotClock {
    fn created_unix_millis(&self) -> u128 {
        now_duration().as_millis()
    }

    fn base_id(&self) -> String {
        let now = now_duration();
        format!("{}-{:09}", now.as_secs(), now.subsec_nanos())
    }
}

fn now_duration() -> std::time::Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

pub struct BackupEngine<'a> {
    root: &'a AppDataRoot,
    catalog: &'a PersistenceCatalog,
}

impl<'a> BackupEngine<'a> {
    pub fn new(root: &'a AppDataRoot, catalog: &'a PersistenceCatalog) -> Self {
        Self { root, catalog }
    }

    pub fn create_snapshot(&self) -> Result<SnapshotResult> {
        self.create_snapshot_with(&SystemSnapshotClock, &RealFileCopier, &|| false)
    }

    pub fn create_snapshot_with_clock(&self, clock: &dyn SnapshotClock) -> Result<SnapshotResult> {
        self.create_snapshot_with(clock, &RealFileCopier, &|| false)
    }

    /// Create a snapshot while cooperatively checking cancellation between
    /// catalog stores and recursively copied filesystem entries.
    pub fn create_snapshot_cancellable(
        &self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<SnapshotResult> {
        self.create_snapshot_with(&SystemSnapshotClock, &RealFileCopier, cancelled)
    }

    /// List only snapshots whose directory and manifest are recognized by the
    /// same validation used for retention. This inspection never creates the
    /// application data or backup directories.
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotRecord>> {
        let backup_root = self.root.path().join(BACKUP_DIRECTORY);
        if !backup_root.exists() {
            return Ok(Vec::new());
        }
        reject_reparse(self.root.path()).context("validate application data root")?;
        reject_reparse(&backup_root).context("validate backup directory")?;
        let canonical_root =
            fs::canonicalize(self.root.path()).context("resolve application data root")?;
        let canonical_backup =
            fs::canonicalize(&backup_root).context("resolve backup directory")?;
        if !canonical_backup.starts_with(&canonical_root) {
            bail!("backup directory escapes application data root");
        }
        Ok(recognized_snapshots(&canonical_backup)?
            .into_iter()
            .take(SNAPSHOT_RETENTION_LIMIT)
            .map(|(_, _, path, manifest)| SnapshotRecord { path, manifest })
            .collect())
    }

    fn create_snapshot_with(
        &self,
        clock: &dyn SnapshotClock,
        copier: &dyn FileCopier,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<SnapshotResult> {
        fs::create_dir_all(self.root.path()).context("create application data root")?;
        reject_reparse(self.root.path()).context("validate application data root")?;
        let canonical_root =
            fs::canonicalize(self.root.path()).context("resolve application data root")?;
        let backup_root = canonical_root.join(BACKUP_DIRECTORY);
        if backup_root.exists() {
            reject_reparse(&backup_root).context("validate backup directory")?;
        } else {
            fs::create_dir(&backup_root).context("create backup directory")?;
        }
        ensure_canonical_within(&canonical_root, &backup_root)?;

        let base_id = validate_id(&clock.base_id())?;
        let (snapshot_id, staging, final_path) = reserve_staging(&backup_root, &base_id)?;
        let result = self.build_snapshot(
            &canonical_root,
            &backup_root,
            &staging,
            &snapshot_id,
            clock.created_unix_millis(),
            copier,
            cancelled,
        );
        let manifest = match result {
            Ok(manifest) => manifest,
            Err(error) => {
                cleanup_owned_staging(&backup_root, &staging);
                return Err(error);
            }
        };

        if let Err(error) = fs::rename(&staging, &final_path) {
            cleanup_owned_staging(&backup_root, &staging);
            return Err(error).with_context(|| {
                format!(
                    "finalize snapshot {} as {}",
                    staging.display(),
                    final_path.display()
                )
            });
        }
        let retention_warning = prune_recognized_snapshots(&backup_root)
            .err()
            .map(|error| error.to_string());
        Ok(SnapshotResult {
            path: final_path,
            manifest,
            retention_warning,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_snapshot(
        &self,
        canonical_root: &Path,
        backup_root: &Path,
        staging: &Path,
        snapshot_id: &str,
        created_unix_millis: u128,
        copier: &dyn FileCopier,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<SnapshotManifest> {
        let mut manifest = SnapshotManifest {
            product: PRODUCT.into(),
            format_version: BACKUP_FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
            snapshot_id: snapshot_id.into(),
            created_unix_millis,
            source_root: canonical_root.to_path_buf(),
            consistency: "PointInTimePerFileNotCrossStoreTransactional".into(),
            status: SnapshotStatus::Complete,
            included: Vec::new(),
            missing: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
            external: Vec::new(),
        };
        let mut operational_issue = false;

        for store in self.catalog.stores() {
            if cancelled() {
                bail!("snapshot creation cancelled");
            }
            if store.backup_policy != BackupPolicy::Include {
                let entry = entry(store, None, Some(policy_reason(store.backup_policy)));
                if store.backup_policy == BackupPolicy::ExcludeExternal {
                    manifest.external.push(entry);
                } else {
                    manifest.skipped.push(entry);
                }
                continue;
            }

            let relative = PathBuf::from("stores").join(format!("{:?}", store.id));
            let destination = staging.join(&relative);
            match validate_source(canonical_root, backup_root, &store.path) {
                Ok(SourceState::Missing) => manifest.missing.push(entry(
                    store,
                    Some(relative),
                    Some("optional source is absent"),
                )),
                Err(error) => {
                    operational_issue = true;
                    manifest.skipped.push(entry(
                        store,
                        Some(relative),
                        Some(format!("unsafe source rejected: {error}")),
                    ));
                }
                Ok(SourceState::Present(metadata)) => {
                    if !kind_matches(store.kind, &metadata) {
                        operational_issue = true;
                        manifest.failed.push(entry(
                            store,
                            Some(relative),
                            Some("source kind does not match catalog descriptor"),
                        ));
                        continue;
                    }
                    let before = (manifest.failed.len(), manifest.skipped.len());
                    copy_source(
                        store,
                        &store.path,
                        &destination,
                        staging,
                        copier,
                        &mut manifest,
                        cancelled,
                    )?;
                    operational_issue |= before != (manifest.failed.len(), manifest.skipped.len());
                }
            }
        }

        manifest.status = if operational_issue && manifest.included.is_empty() {
            SnapshotStatus::Failed
        } else if operational_issue {
            SnapshotStatus::Partial
        } else {
            SnapshotStatus::Complete
        };
        save_json_atomic(&staging.join(MANIFEST_FILE), &manifest)
            .context("write durable snapshot manifest")?;
        Ok(manifest)
    }
}

enum SourceState {
    Missing,
    Present(Metadata),
}

fn validate_source(root: &Path, backup_root: &Path, source: &Path) -> Result<SourceState> {
    reject_lexical_traversal(source)?;
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SourceState::Missing);
        }
        Err(error) => return Err(error).with_context(|| format!("inspect {}", source.display())),
    };
    reject_reparse_metadata(source, &metadata)?;
    let canonical =
        fs::canonicalize(source).with_context(|| format!("resolve {}", source.display()))?;
    if !canonical.starts_with(root) {
        bail!("source escapes application data root");
    }
    reject_reparse_chain(root, &canonical)?;
    if backup_root.starts_with(&canonical) {
        bail!("source contains the backup destination");
    }
    Ok(SourceState::Present(metadata))
}

fn copy_source(
    store: &StoreDescriptor,
    source: &Path,
    destination: &Path,
    staging: &Path,
    copier: &dyn FileCopier,
    manifest: &mut SnapshotManifest,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    if cancelled() {
        bail!("snapshot creation cancelled");
    }
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) => {
            manifest.failed.push(entry_at(
                store,
                source,
                destination
                    .strip_prefix(staging)
                    .ok()
                    .map(Path::to_path_buf),
                Some(format!("inspect failed: {error}")),
            ));
            return Ok(());
        }
    };
    if let Err(error) = reject_reparse_metadata(source, &metadata) {
        manifest.skipped.push(entry_at(
            store,
            source,
            destination
                .strip_prefix(staging)
                .ok()
                .map(Path::to_path_buf),
            Some(format!("unsafe entry rejected: {error}")),
        ));
        return Ok(());
    }
    if metadata.is_dir() {
        if let Err(error) = fs::create_dir_all(destination) {
            manifest.failed.push(entry_at(
                store,
                source,
                destination
                    .strip_prefix(staging)
                    .ok()
                    .map(Path::to_path_buf),
                Some(format!("create destination failed: {error}")),
            ));
            return Ok(());
        }
        let entries = match fs::read_dir(source) {
            Ok(entries) => entries,
            Err(error) => {
                manifest.failed.push(entry_at(
                    store,
                    source,
                    destination
                        .strip_prefix(staging)
                        .ok()
                        .map(Path::to_path_buf),
                    Some(format!("read directory failed: {error}")),
                ));
                return Ok(());
            }
        };
        let mut found = false;
        for child in entries {
            found = true;
            match child {
                Ok(child) => {
                    let name = child.file_name();
                    if Path::new(&name).components().count() != 1
                        || !matches!(
                            Path::new(&name).components().next(),
                            Some(Component::Normal(_))
                        )
                    {
                        manifest.skipped.push(entry_at(
                            store,
                            source,
                            None,
                            Some("directory traversal entry rejected"),
                        ));
                        continue;
                    }
                    copy_source(
                        store,
                        &child.path(),
                        &destination.join(name),
                        staging,
                        copier,
                        manifest,
                        cancelled,
                    )?;
                }
                Err(error) => manifest.failed.push(entry_at(
                    store,
                    source,
                    destination
                        .strip_prefix(staging)
                        .ok()
                        .map(Path::to_path_buf),
                    Some(format!("enumeration failed: {error}")),
                )),
            }
        }
        if !found {
            manifest.included.push(entry_at(
                store,
                source,
                destination
                    .strip_prefix(staging)
                    .ok()
                    .map(Path::to_path_buf),
                Some("empty directory"),
            ));
        }
    } else if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                manifest.failed.push(entry_at(
                    store,
                    source,
                    destination
                        .strip_prefix(staging)
                        .ok()
                        .map(Path::to_path_buf),
                    Some(format!("create destination failed: {error}")),
                ));
                return Ok(());
            }
        }
        match copier.copy(source, destination) {
            Ok(()) => manifest.included.push(entry_at(
                store,
                source,
                destination
                    .strip_prefix(staging)
                    .ok()
                    .map(Path::to_path_buf),
                None::<String>,
            )),
            Err(error) => {
                // A failed platform copy may leave a truncated destination.
                // Failed entries are evidence only, never restorable bytes.
                let _ = fs::remove_file(destination);
                manifest.failed.push(entry_at(
                    store,
                    source,
                    destination
                        .strip_prefix(staging)
                        .ok()
                        .map(Path::to_path_buf),
                    Some(format!("copy failed: {error}")),
                ));
            }
        }
    } else {
        manifest.skipped.push(entry_at(
            store,
            source,
            destination
                .strip_prefix(staging)
                .ok()
                .map(Path::to_path_buf),
            Some("unsupported filesystem entry"),
        ));
    }
    Ok(())
}

trait FileCopier {
    fn copy(&self, source: &Path, destination: &Path) -> std::io::Result<()>;
}

struct RealFileCopier;

impl FileCopier for RealFileCopier {
    fn copy(&self, source: &Path, destination: &Path) -> std::io::Result<()> {
        fs::copy(source, destination).map(|_| ())
    }
}

fn entry(
    store: &StoreDescriptor,
    snapshot_path: Option<PathBuf>,
    detail: Option<impl Into<String>>,
) -> SnapshotEntry {
    entry_at(store, &store.path, snapshot_path, detail)
}

fn entry_at(
    store: &StoreDescriptor,
    source_path: &Path,
    snapshot_path: Option<PathBuf>,
    detail: Option<impl Into<String>>,
) -> SnapshotEntry {
    SnapshotEntry {
        store_id: format!("{:?}", store.id),
        source_path: source_path.to_path_buf(),
        snapshot_path,
        detail: detail.map(Into::into),
    }
}

fn policy_reason(policy: BackupPolicy) -> &'static str {
    match policy {
        BackupPolicy::Include => "included",
        BackupPolicy::ExcludeExternal => "external source excluded",
        BackupPolicy::ExcludeReplaceable => "replaceable or private data excluded",
        BackupPolicy::ExcludeRuntime => "runtime data excluded",
    }
}

fn kind_matches(kind: StoreKind, metadata: &Metadata) -> bool {
    matches!(
        (kind, metadata.is_file(), metadata.is_dir()),
        (StoreKind::File, true, _) | (StoreKind::Directory, _, true)
    )
}

fn reserve_staging(backup_root: &Path, base_id: &str) -> Result<(String, PathBuf, PathBuf)> {
    for suffix in 0..1000u32 {
        let id = if suffix == 0 {
            base_id.to_owned()
        } else {
            format!("{base_id}-{suffix}")
        };
        let staging = backup_root.join(format!("{STAGING_PREFIX}{id}"));
        let final_path = backup_root.join(&id);
        if final_path.exists() || staging.exists() {
            continue;
        }
        match fs::create_dir(&staging) {
            Ok(()) => return Ok((id, staging, final_path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("reserve backup staging directory"),
        }
    }
    bail!("unable to reserve a collision-safe snapshot ID")
}

fn validate_id(id: &str) -> Result<String> {
    if id.is_empty()
        || id.len() > 120
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("snapshot ID contains unsafe path characters");
    }
    Ok(id.to_owned())
}

fn prune_recognized_snapshots(backup_root: &Path) -> Result<()> {
    let recognized = recognized_snapshots(backup_root)?;
    for (_, _, path, _) in recognized.into_iter().skip(SNAPSHOT_RETENTION_LIMIT) {
        fs::remove_dir_all(&path).with_context(|| format!("prune snapshot {}", path.display()))?;
    }
    Ok(())
}

fn recognized_snapshots(
    backup_root: &Path,
) -> Result<Vec<(u128, String, PathBuf, SnapshotManifest)>> {
    let mut recognized = Vec::new();
    for item in fs::read_dir(backup_root).context("enumerate backup retention directory")? {
        let item = item?;
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with(STAGING_PREFIX) || reject_reparse(&path).is_err() || !path.is_dir() {
            continue;
        }
        let Ok(bytes) = fs::read(path.join(MANIFEST_FILE)) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_slice::<SnapshotManifest>(&bytes) else {
            continue;
        };
        if manifest.product == PRODUCT
            && manifest.format_version == BACKUP_FORMAT_VERSION
            && manifest.snapshot_id == name
            && validate_id(&name).is_ok()
            && tree_is_reparse_free(&path)
            && fs::canonicalize(&path).is_ok_and(|resolved| resolved.starts_with(backup_root))
        {
            recognized.push((manifest.created_unix_millis, name, path, manifest));
        }
    }
    recognized.sort_by(|left, right| (right.0, &right.1).cmp(&(left.0, &left.1)));
    Ok(recognized)
}

/// Cleanup is deliberately best-effort and refuses anything whose identity is
/// no longer the exact staging tree reserved by this engine. A crash artifact
/// or suspicious replacement stays unrecognized for explicit inspection.
fn cleanup_owned_staging(backup_root: &Path, staging: &Path) {
    let owned_name = staging
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(STAGING_PREFIX));
    let contained = fs::canonicalize(backup_root).is_ok_and(|canonical_backup_root| {
        fs::canonicalize(staging).is_ok_and(|resolved| resolved.starts_with(canonical_backup_root))
    });
    if owned_name && contained && tree_is_reparse_free(staging) {
        let _ = fs::remove_dir_all(staging);
    }
}

fn tree_is_reparse_free(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if is_reparse(&metadata) {
        return false;
    }
    if metadata.is_dir() {
        let Ok(entries) = fs::read_dir(path) else {
            return false;
        };
        for entry in entries {
            let Ok(entry) = entry else { return false };
            if !tree_is_reparse_free(&entry.path()) {
                return false;
            }
        }
    }
    true
}

fn reject_lexical_traversal(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("path contains parent traversal");
    }
    Ok(())
}

fn ensure_canonical_within(root: &Path, path: &Path) -> Result<()> {
    let canonical =
        fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))?;
    if !canonical.starts_with(root) {
        bail!("path escapes application data root");
    }
    reject_reparse_chain(root, &canonical)
}

fn reject_reparse_chain(root: &Path, path: &Path) -> Result<()> {
    let relative = path.strip_prefix(root).context("path escapes owned root")?;
    let mut current = root.to_path_buf();
    reject_reparse(&current)?;
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!("unsafe path component");
        }
        current.push(component.as_os_str());
        reject_reparse(&current)?;
    }
    Ok(())
}

fn reject_reparse(path: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    reject_reparse_metadata(path, &metadata)
}

fn reject_reparse_metadata(path: &Path, metadata: &Metadata) -> Result<()> {
    if is_reparse(metadata) {
        bail!("{} is a symlink or reparse point", path.display());
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{StoreOwnership, StorePrivacy, WriteFrequency};
    use crate::settings::Settings;
    use std::cell::Cell;

    struct FixedClock {
        millis: u128,
        id: String,
    }
    impl SnapshotClock for FixedClock {
        fn created_unix_millis(&self) -> u128 {
            self.millis
        }
        fn base_id(&self) -> String {
            self.id.clone()
        }
    }

    struct FailingCopier {
        calls: Cell<usize>,
        fail_at: usize,
    }
    impl FileCopier for FailingCopier {
        fn copy(&self, source: &Path, destination: &Path) -> std::io::Result<()> {
            let call = self.calls.get();
            self.calls.set(call + 1);
            if call == self.fail_at {
                fs::write(destination, b"partial")?;
                Err(std::io::Error::other("injected copy failure"))
            } else {
                fs::copy(source, destination).map(|_| ())
            }
        }
    }

    struct CancellingCopier<'a> {
        cancelled: &'a Cell<bool>,
    }
    impl FileCopier for CancellingCopier<'_> {
        fn copy(&self, source: &Path, destination: &Path) -> std::io::Result<()> {
            fs::copy(source, destination)?;
            self.cancelled.set(true);
            Ok(())
        }
    }

    fn fixture() -> (tempfile::TempDir, AppDataRoot, PersistenceCatalog) {
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let base = PersistenceCatalog::new(&root, &Settings::default());
        let mut settings = base.get(PersistentStoreId::Settings).clone();
        settings.path = dir.path().join("settings.json");
        settings.backup_policy = BackupPolicy::Include;
        settings.ownership = StoreOwnership::ApplicationOwned;
        let mut missing = settings.clone();
        missing.id = PersistentStoreId::Actions;
        missing.path = dir.path().join("optional.json");
        (
            dir,
            root,
            PersistenceCatalog::from_stores(vec![settings, missing]),
        )
    }

    fn derived_store(
        base: &StoreDescriptor,
        id: PersistentStoreId,
        path: PathBuf,
        kind: StoreKind,
    ) -> StoreDescriptor {
        let mut store = base.clone();
        store.id = id;
        store.path = path;
        store.kind = kind;
        store.backup_policy = BackupPolicy::Include;
        store.ownership = StoreOwnership::ApplicationOwned;
        store.privacy = StorePrivacy::Sensitive;
        store.frequency = WriteFrequency::Moderate;
        store
    }

    #[test]
    fn manifest_and_copied_bytes_are_durable_and_missing_is_recorded() {
        let (dir, root, catalog) = fixture();
        fs::write(dir.path().join("settings.json"), b"exact bytes").unwrap();
        let result = BackupEngine::new(&root, &catalog)
            .create_snapshot_with_clock(&FixedClock {
                millis: 7,
                id: "fixed".into(),
            })
            .unwrap();
        assert_eq!(
            fs::read(result.path.join("stores/Settings")).unwrap(),
            b"exact bytes"
        );
        assert_eq!(result.manifest.status, SnapshotStatus::Complete);
        assert_eq!(result.retention_warning, None);
        assert_eq!(result.manifest.missing.len(), 1);
        assert_eq!(
            result.manifest.consistency,
            "PointInTimePerFileNotCrossStoreTransactional"
        );
        let disk: SnapshotManifest =
            serde_json::from_slice(&fs::read(result.path.join(MANIFEST_FILE)).unwrap()).unwrap();
        assert_eq!(disk, result.manifest);
        assert_eq!(
            disk.included[0].source_path,
            dir.path().join("settings.json")
        );

        let listed = BackupEngine::new(&root, &catalog).list_snapshots().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, result.path);
        assert_eq!(listed[0].manifest, result.manifest);
    }

    #[test]
    fn cancellation_between_files_removes_current_staging_tree() {
        let (dir, root, catalog) = fixture();
        fs::write(dir.path().join("settings.json"), b"settings").unwrap();
        fs::write(dir.path().join("optional.json"), b"actions").unwrap();
        let cancelled = Cell::new(false);
        let result = BackupEngine::new(&root, &catalog).create_snapshot_with(
            &FixedClock {
                millis: 1,
                id: "cancelled".into(),
            },
            &CancellingCopier {
                cancelled: &cancelled,
            },
            &|| cancelled.get(),
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        let backup_root = dir.path().join(BACKUP_DIRECTORY);
        assert!(!backup_root.join("cancelled").exists());
        assert!(
            !backup_root
                .join(format!("{STAGING_PREFIX}cancelled"))
                .exists()
        );
    }

    #[test]
    fn catalog_policy_excludes_external_private_replaceable_and_runtime_bytes() {
        let (dir, root, mut catalog) = fixture();
        fs::write(dir.path().join("settings.json"), b"keep").unwrap();
        let outside = tempfile::tempdir().unwrap();
        let mut stores = catalog.stores().to_vec();
        for (id, policy, name) in [
            (
                PersistentStoreId::QueryHistory,
                BackupPolicy::ExcludeReplaceable,
                "private",
            ),
            (
                PersistentStoreId::LauncherLog,
                BackupPolicy::ExcludeRuntime,
                "runtime",
            ),
            (
                PersistentStoreId::Scratchpad,
                BackupPolicy::ExcludeExternal,
                "external",
            ),
        ] {
            let path = outside.path().join(name);
            fs::write(&path, name).unwrap();
            let mut store = stores[0].clone();
            store.id = id;
            store.path = path;
            store.backup_policy = policy;
            stores.push(store);
        }
        catalog = PersistenceCatalog::from_stores(stores);
        let result = BackupEngine::new(&root, &catalog)
            .create_snapshot_with_clock(&FixedClock {
                millis: 1,
                id: "policy".into(),
            })
            .unwrap();
        assert_eq!(result.manifest.external.len(), 1);
        assert_eq!(result.manifest.skipped.len(), 2);
        assert!(
            !fs::read_dir(&result.path)
                .unwrap()
                .any(|entry| entry.unwrap().file_name() == "QueryHistory")
        );
    }

    #[test]
    fn mkmacro_and_internal_notes_assets_are_copied() {
        let (dir, root, catalog) = fixture();
        fs::write(dir.path().join("settings.json"), b"settings").unwrap();
        let assets = dir.path().join("mkmacro_assets");
        let notes_assets = dir.path().join("notes/assets");
        fs::create_dir_all(&assets).unwrap();
        fs::create_dir_all(&notes_assets).unwrap();
        fs::write(assets.join("image.png"), b"macro image").unwrap();
        fs::write(notes_assets.join("note.png"), b"note image").unwrap();
        let base = &catalog.stores()[0];
        let stores = vec![
            derived_store(
                base,
                PersistentStoreId::MkMacroAssets,
                assets,
                StoreKind::Directory,
            ),
            derived_store(
                base,
                PersistentStoreId::NotesAssets,
                notes_assets,
                StoreKind::Directory,
            ),
        ];
        let catalog = PersistenceCatalog::from_stores(stores);
        let result = BackupEngine::new(&root, &catalog)
            .create_snapshot_with_clock(&FixedClock {
                millis: 1,
                id: "assets".into(),
            })
            .unwrap();
        assert_eq!(
            fs::read(result.path.join("stores/MkMacroAssets/image.png")).unwrap(),
            b"macro image"
        );
        assert_eq!(
            fs::read(result.path.join("stores/NotesAssets/note.png")).unwrap(),
            b"note image"
        );
    }

    #[test]
    fn injected_copy_failure_finalizes_an_honest_partial_snapshot() {
        let (dir, root, catalog) = fixture();
        fs::write(dir.path().join("settings.json"), b"settings").unwrap();
        fs::write(dir.path().join("optional.json"), b"action").unwrap();
        let copier = FailingCopier {
            calls: Cell::new(0),
            fail_at: 1,
        };
        let result = BackupEngine::new(&root, &catalog)
            .create_snapshot_with(
                &FixedClock {
                    millis: 1,
                    id: "partial".into(),
                },
                &copier,
                &|| false,
            )
            .unwrap();
        assert_eq!(result.manifest.status, SnapshotStatus::Partial);
        assert_eq!(result.manifest.included.len(), 1);
        assert_eq!(result.manifest.failed.len(), 1);
        assert!(!result.path.join("stores/Actions").exists());
    }

    #[test]
    fn collision_safe_ids_do_not_replace_existing_snapshots() {
        let (dir, root, catalog) = fixture();
        fs::write(dir.path().join("settings.json"), b"one").unwrap();
        let engine = BackupEngine::new(&root, &catalog);
        let first = engine
            .create_snapshot_with_clock(&FixedClock {
                millis: 1,
                id: "same".into(),
            })
            .unwrap();
        fs::write(dir.path().join("settings.json"), b"two").unwrap();
        let second = engine
            .create_snapshot_with_clock(&FixedClock {
                millis: 2,
                id: "same".into(),
            })
            .unwrap();
        assert_eq!(first.path.file_name().unwrap(), "same");
        assert_eq!(second.path.file_name().unwrap(), "same-1");
        assert_eq!(
            fs::read(first.path.join("stores/Settings")).unwrap(),
            b"one"
        );
    }

    #[test]
    fn retention_keeps_newest_five_and_never_touches_unknown_or_staging_directories() {
        let (dir, root, catalog) = fixture();
        fs::write(dir.path().join("settings.json"), b"settings").unwrap();
        let backup_root = dir.path().join(BACKUP_DIRECTORY);
        fs::create_dir_all(backup_root.join("unknown")).unwrap();
        fs::write(backup_root.join("unknown/keep.txt"), b"keep").unwrap();
        fs::create_dir_all(backup_root.join(format!("{STAGING_PREFIX}abandoned"))).unwrap();
        let engine = BackupEngine::new(&root, &catalog);
        for index in 0..7 {
            engine
                .create_snapshot_with_clock(&FixedClock {
                    millis: index,
                    id: format!("snapshot-{index}"),
                })
                .unwrap();
        }
        let recognized = fs::read_dir(&backup_root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().join(MANIFEST_FILE).is_file())
            .count();
        assert_eq!(recognized, 5);
        assert!(backup_root.join("unknown/keep.txt").is_file());
        assert!(
            backup_root
                .join(format!("{STAGING_PREFIX}abandoned"))
                .is_dir()
        );
        assert!(!backup_root.join("snapshot-0").exists());
        assert!(backup_root.join("snapshot-6").exists());
    }

    #[test]
    fn cleanup_removes_only_safe_engine_owned_staging_trees() {
        let dir = tempfile::tempdir().unwrap();
        let backup_root = dir.path();
        let staging = backup_root.join(format!("{STAGING_PREFIX}current"));
        let unknown = backup_root.join("unknown-staging");
        fs::create_dir(&staging).unwrap();
        fs::create_dir(&unknown).unwrap();
        cleanup_owned_staging(backup_root, &unknown);
        cleanup_owned_staging(backup_root, &staging);
        assert!(!staging.exists());
        assert!(unknown.exists());
    }

    #[test]
    fn traversal_ids_and_sources_containing_destination_are_rejected() {
        let (dir, root, catalog) = fixture();
        assert!(
            BackupEngine::new(&root, &catalog)
                .create_snapshot_with_clock(&FixedClock {
                    millis: 1,
                    id: "../escape".into()
                })
                .is_err()
        );
        let mut store = catalog.stores()[0].clone();
        store.path = dir.path().to_path_buf();
        store.kind = StoreKind::Directory;
        let catalog = PersistenceCatalog::from_stores(vec![store]);
        let result = BackupEngine::new(&root, &catalog)
            .create_snapshot_with_clock(&FixedClock {
                millis: 1,
                id: "safe".into(),
            })
            .unwrap();
        assert_eq!(result.manifest.status, SnapshotStatus::Failed);
        assert!(
            result.manifest.skipped[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("backup destination")
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn symlink_sources_and_entries_are_rejected_without_copying_targets() {
        let (dir, root, catalog) = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), b"secret").unwrap();
        let source = dir.path().join("assets");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("ordinary"), b"ordinary").unwrap();
        let link = source.join("link");
        let source_link = dir.path().join("source-link");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path().join("secret"), &link).unwrap();
            std::os::unix::fs::symlink(outside.path().join("secret"), &source_link).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(outside.path().join("secret"), &link).unwrap();
            std::os::windows::fs::symlink_file(outside.path().join("secret"), &source_link)
                .unwrap();
        }
        let assets = derived_store(
            &catalog.stores()[0],
            PersistentStoreId::Actions,
            source,
            StoreKind::Directory,
        );
        let linked_source = derived_store(
            &catalog.stores()[0],
            PersistentStoreId::Bookmarks,
            source_link,
            StoreKind::File,
        );
        let catalog = PersistenceCatalog::from_stores(vec![assets, linked_source]);
        let result = BackupEngine::new(&root, &catalog)
            .create_snapshot_with_clock(&FixedClock {
                millis: 1,
                id: "links".into(),
            })
            .unwrap();
        assert_eq!(result.manifest.status, SnapshotStatus::Partial);
        assert_eq!(result.manifest.included.len(), 1);
        assert_eq!(result.manifest.skipped.len(), 2);
        assert_eq!(
            fs::read(result.path.join("stores/Actions/ordinary")).unwrap(),
            b"ordinary"
        );
        assert!(!result.path.join("stores/Actions/link").exists());
    }
}
