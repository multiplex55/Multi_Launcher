use super::{
    BackupEngine, PersistenceCatalog, PersistentStoreId, SnapshotManifest, StoreDescriptor,
    StoreHealth, StoreKind, StoreOwnership,
};
use crate::actions::Action;
use crate::common::atomic_file::{backup_file, save_atomic};
use crate::dashboard::config::DashboardConfig;
use crate::history::{HistoryEntry, HistoryPin};
use crate::mkmacro::model::MkMacroDocument;
use crate::mouse_gestures::{db::GestureDb, usage::GestureUsageEntry};
use crate::multi_manager::bindings::WorkspaceBindingSnapshot;
use crate::multi_manager::model::MmWorkspace;
use crate::note_ui_state::NoteUiState;
use crate::platform::app_data::AppDataRoot;
use crate::plugins::bookmarks::BookmarkEntry;
use crate::plugins::calc_history::CalcHistoryEntry;
use crate::plugins::calendar::{CalendarEvent, CalendarState};
use crate::plugins::fav::FavEntry;
use crate::plugins::folders::default_folders;
use crate::plugins::layouts_storage::LayoutStore;
use crate::plugins::macros::MacroEntry;
use crate::plugins::shell::ShellCmdEntry;
use crate::plugins::snippets::SnippetEntry;
use crate::plugins::todo::TodoEntry;
use crate::settings::Settings;
use crate::usage::UsageEntry;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RECOVERY_DIRECTORY: &str = "recovery";
const PENDING_FILE: &str = "pending.json";
const RECOVERY_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum StagedRecoveryAction {
    Restore {
        store_id: PersistentStoreId,
        snapshot_id: String,
    },
    Reset {
        store_id: PersistentStoreId,
    },
}

impl StagedRecoveryAction {
    pub fn store_id(&self) -> PersistentStoreId {
        match self {
            Self::Restore { store_id, .. } | Self::Reset { store_id } => *store_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRecoveryDescriptor {
    pub format_version: u32,
    pub action: StagedRecoveryAction,
    /// Fingerprint of the fully validated candidate at staging time. Startup
    /// recomputes it before touching the live destination.
    pub candidate_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryStartupDiagnostic {
    InvalidPending {
        message: String,
    },
    ApplyFailed {
        action: Option<StagedRecoveryAction>,
        message: String,
    },
}

impl std::fmt::Display for RecoveryStartupDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPending { message } => {
                write!(formatter, "invalid pending recovery: {message}")
            }
            Self::ApplyFailed { message, .. } => {
                write!(formatter, "pending recovery failed: {message}")
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveryStartupResult {
    pub applied: Option<StagedRecoveryAction>,
    pub diagnostic: Option<RecoveryStartupDiagnostic>,
}

pub struct RecoveryManager<'a> {
    root: &'a AppDataRoot,
    catalog: &'a PersistenceCatalog,
}

impl<'a> RecoveryManager<'a> {
    pub fn new(root: &'a AppDataRoot, catalog: &'a PersistenceCatalog) -> Self {
        Self { root, catalog }
    }

    pub fn pending_path(&self) -> PathBuf {
        self.root.path().join(RECOVERY_DIRECTORY).join(PENDING_FILE)
    }

    pub fn stage_restore(
        &self,
        store_id: PersistentStoreId,
        snapshot_id: &str,
    ) -> Result<PendingRecoveryDescriptor> {
        let descriptor = self.eligible_store(store_id, true)?;
        self.ensure_bootstrap_target(descriptor)?;
        let candidate = self.restore_candidate(descriptor, snapshot_id)?;
        ensure_healthy(descriptor, &candidate.path)?;
        let pending = PendingRecoveryDescriptor {
            format_version: RECOVERY_FORMAT_VERSION,
            action: StagedRecoveryAction::Restore {
                store_id,
                snapshot_id: snapshot_id.to_owned(),
            },
            candidate_fingerprint: fingerprint(&candidate.path)?,
        };
        self.write_pending(&pending)?;
        Ok(pending)
    }

    pub fn stage_reset(&self, store_id: PersistentStoreId) -> Result<PendingRecoveryDescriptor> {
        let descriptor = self.eligible_store(store_id, false)?;
        self.ensure_bootstrap_target(descriptor)?;
        let candidate = canonical_reset(store_id, descriptor.kind)?;
        let pending = PendingRecoveryDescriptor {
            format_version: RECOVERY_FORMAT_VERSION,
            action: StagedRecoveryAction::Reset { store_id },
            candidate_fingerprint: candidate.fingerprint()?,
        };
        self.write_pending(&pending)?;
        Ok(pending)
    }

    fn write_pending(&self, pending: &PendingRecoveryDescriptor) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(pending).context("serialize pending recovery")?;
        let path = validated_pending_path(self.root, true)?;
        save_atomic(&path, &bytes).context("atomically stage pending recovery")
    }

    fn eligible_store(&self, id: PersistentStoreId, restore: bool) -> Result<&StoreDescriptor> {
        let store = self.catalog.get(id);
        ensure!(
            store.ownership == StoreOwnership::ApplicationOwned,
            "{} is not application-owned",
            store.label
        );
        ensure!(
            if restore {
                store.restore_eligible
            } else {
                store.reset_eligible
            },
            "{} is not eligible for {}",
            store.label,
            if restore { "restore" } else { "reset" }
        );
        validate_owned_destination(self.root, &store.path)?;
        Ok(store)
    }

    /// Startup cannot trust a destination persisted in the pending request and
    /// intentionally runs before settings are loaded. Therefore recovery is
    /// staged only when the typed ID resolves to the same target in the
    /// settings-free bootstrap catalog.
    fn ensure_bootstrap_target(&self, store: &StoreDescriptor) -> Result<()> {
        let bootstrap = PersistenceCatalog::bootstrap(self.root);
        let expected = bootstrap.get(store.id);
        ensure!(
            expected.ownership == StoreOwnership::ApplicationOwned
                && paths_equal(&expected.path, &store.path),
            "{} uses a configured target unavailable before settings load",
            store.label
        );
        Ok(())
    }

    fn restore_candidate(&self, store: &StoreDescriptor, snapshot_id: &str) -> Result<Candidate> {
        validate_snapshot_id(snapshot_id)?;
        let snapshot = BackupEngine::new(self.root, self.catalog)
            .list_snapshots()?
            .into_iter()
            .find(|record| record.manifest.snapshot_id == snapshot_id)
            .context("snapshot ID is not recognized")?;
        validate_manifest_root(self.root, &snapshot.manifest)?;
        selected_candidate(store, &snapshot.path, &snapshot.manifest)
    }
}

/// Apply the short-lived instruction after single-instance ownership is held
/// and before settings, plugins, watchers, or other persistent stores load.
pub fn apply_pending_recovery(root: &AppDataRoot) -> RecoveryStartupResult {
    let pending_path = match validated_pending_path(root, false) {
        Ok(path) => path,
        Err(_error) if !root.path().join(RECOVERY_DIRECTORY).exists() => return Default::default(),
        Err(error) => {
            return RecoveryStartupResult {
                diagnostic: Some(RecoveryStartupDiagnostic::InvalidPending {
                    message: error.to_string(),
                }),
                ..Default::default()
            };
        }
    };
    let bytes = match fs::read(&pending_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Default::default(),
        Err(error) => {
            return RecoveryStartupResult {
                diagnostic: Some(RecoveryStartupDiagnostic::InvalidPending {
                    message: error.to_string(),
                }),
                ..Default::default()
            };
        }
    };
    let pending = match serde_json::from_slice::<PendingRecoveryDescriptor>(&bytes) {
        Ok(pending) if pending.format_version == RECOVERY_FORMAT_VERSION => pending,
        Ok(pending) => {
            return RecoveryStartupResult {
                diagnostic: Some(RecoveryStartupDiagnostic::InvalidPending {
                    message: format!("unsupported descriptor version {}", pending.format_version),
                }),
                ..Default::default()
            };
        }
        Err(error) => {
            return RecoveryStartupResult {
                diagnostic: Some(RecoveryStartupDiagnostic::InvalidPending {
                    message: error.to_string(),
                }),
                ..Default::default()
            };
        }
    };
    let catalog = PersistenceCatalog::bootstrap(root);
    let manager = RecoveryManager::new(root, &catalog);
    match apply_validated(&manager, &pending) {
        Ok(()) => RecoveryStartupResult {
            applied: Some(pending.action),
            diagnostic: None,
        },
        Err(error) => RecoveryStartupResult {
            diagnostic: Some(RecoveryStartupDiagnostic::ApplyFailed {
                action: Some(pending.action),
                message: error.to_string(),
            }),
            ..Default::default()
        },
    }
}

fn apply_validated(
    manager: &RecoveryManager<'_>,
    pending: &PendingRecoveryDescriptor,
) -> Result<()> {
    let store = match &pending.action {
        StagedRecoveryAction::Restore { store_id, .. } => {
            manager.eligible_store(*store_id, true)?
        }
        StagedRecoveryAction::Reset { store_id } => manager.eligible_store(*store_id, false)?,
    };
    manager.ensure_bootstrap_target(store)?;

    match &pending.action {
        StagedRecoveryAction::Restore { snapshot_id, .. } => {
            let candidate = manager.restore_candidate(store, snapshot_id)?;
            apply_restore_with_hook(store, &candidate, &pending.candidate_fingerprint, &|| {})?;
        }
        StagedRecoveryAction::Reset { store_id } => {
            let candidate = canonical_reset(*store_id, store.kind)?;
            ensure!(
                candidate.fingerprint()? == pending.candidate_fingerprint,
                "canonical reset candidate does not match staged request"
            );
            install_reset(&store.path, &candidate, "pre-reset")?;
        }
    }
    fs::remove_file(manager.pending_path()).context("clear completed pending recovery")?;
    Ok(())
}

struct Candidate {
    path: PathBuf,
    kind: StoreKind,
}

enum ResetCandidate {
    File(Vec<u8>),
    EmptyDirectory,
}

impl ResetCandidate {
    fn fingerprint(&self) -> Result<String> {
        match self {
            Self::File(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(bytes)
                    .context("parse canonical reset JSON for fingerprint")?;
                let mut hash = Fnv64::default();
                hash.update(&[b'F']);
                hash_json(&value, &mut hash);
                Ok(format!("fnv1a64:{:016x}", hash.0))
            }
            Self::EmptyDirectory => Ok(fingerprint_bytes(b'D', &[])),
        }
    }
}

fn selected_candidate(
    store: &StoreDescriptor,
    snapshot_root: &Path,
    manifest: &SnapshotManifest,
) -> Result<Candidate> {
    let store_name = format!("{:?}", store.id);
    ensure!(
        !manifest
            .failed
            .iter()
            .any(|entry| entry.store_id == store_name)
            && !manifest
                .skipped
                .iter()
                .any(|entry| entry.store_id == store_name),
        "snapshot did not capture the selected store completely"
    );
    let relative = PathBuf::from("stores").join(&store_name);
    let selected = manifest
        .included
        .iter()
        .filter(|entry| entry.store_id == store_name)
        .collect::<Vec<_>>();
    ensure!(
        !selected.is_empty(),
        "selected store is not included in snapshot"
    );
    for entry in &selected {
        let path = entry
            .snapshot_path
            .as_ref()
            .context("included entry has no snapshot path")?;
        ensure!(
            safe_relative(path),
            "included entry has an unsafe snapshot path"
        );
        ensure!(
            path == &relative || path.starts_with(&relative),
            "included entry escapes selected store"
        );
    }
    let path = snapshot_root.join(&relative);
    validate_snapshot_candidate(snapshot_root, &path, store.kind)?;
    if store.kind == StoreKind::File {
        ensure!(
            selected
                .iter()
                .any(|entry| entry.snapshot_path.as_ref() == Some(&relative)),
            "snapshot manifest does not identify the selected file"
        );
    } else {
        let recorded = selected
            .iter()
            .filter_map(|entry| entry.snapshot_path.clone())
            .collect::<BTreeSet<_>>();
        for file in regular_tree_paths(&path)? {
            let manifest_path = relative.join(file);
            ensure!(
                recorded.contains(&manifest_path),
                "snapshot contains an unmanifested file"
            );
        }
    }
    Ok(Candidate {
        path,
        kind: store.kind,
    })
}

fn validate_manifest_root(root: &AppDataRoot, manifest: &SnapshotManifest) -> Result<()> {
    let canonical_root = fs::canonicalize(root.path()).context("resolve application data root")?;
    let source_root =
        fs::canonicalize(&manifest.source_root).context("resolve manifest source root")?;
    ensure!(
        paths_equal(&canonical_root, &source_root),
        "snapshot belongs to another data root"
    );
    Ok(())
}

fn ensure_healthy(store: &StoreDescriptor, candidate: &Path) -> Result<()> {
    match store.probe_path(candidate) {
        StoreHealth::Healthy | StoreHealth::Empty => Ok(()),
        health => bail!(
            "candidate failed the {} domain probe: {health:?}",
            store.label
        ),
    }
}

fn apply_restore_with_hook(
    store: &StoreDescriptor,
    candidate: &Candidate,
    expected_fingerprint: &str,
    before_install: &dyn Fn(),
) -> Result<()> {
    match candidate.kind {
        StoreKind::File => {
            let bytes = fs::read(&candidate.path).context("materialize recovery file")?;
            ensure!(
                fingerprint_bytes(b'F', &bytes) == expected_fingerprint,
                "staged recovery bytes changed after validation"
            );
            let parent = store
                .path
                .parent()
                .context("recovery file destination has no parent")?;
            fs::create_dir_all(parent).context("create recovery destination parent")?;
            let name = store
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("store");
            let staging =
                reserve_staging_file(parent, &format!(".{name}.recovery-staging"), &bytes)?;
            let result = (|| {
                ensure!(
                    fingerprint(&staging)? == expected_fingerprint,
                    "materialized recovery file does not match staged request"
                );
                ensure_healthy(store, &staging)?;
                ensure!(
                    fingerprint(&staging)? == expected_fingerprint,
                    "materialized recovery file changed during validation"
                );
                before_install();
                install_file_with(&store.path, &bytes, "pre-restore", save_atomic)
                    .context("atomically install recovered file")
            })();
            cleanup_created_file(parent, &staging);
            result
        }
        StoreKind::Directory => {
            let parent = store
                .path
                .parent()
                .context("recovery directory destination has no parent")?;
            fs::create_dir_all(parent).context("create recovery destination parent")?;
            let name = store
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("store");
            let staging = reserve_sibling(parent, &format!(".{name}.recovery-staging"))?;
            if let Err(error) = copy_tree(&candidate.path, &staging) {
                cleanup_created_directory(parent, &staging);
                return Err(error).context("materialize recovery directory");
            }
            let result = (|| {
                ensure!(
                    fingerprint(&staging)? == expected_fingerprint,
                    "materialized recovery directory does not match staged request"
                );
                ensure_healthy(store, &staging)?;
                ensure!(
                    fingerprint(&staging)? == expected_fingerprint,
                    "materialized recovery directory changed during validation"
                );
                before_install();
                ensure!(
                    fingerprint(&staging)? == expected_fingerprint,
                    "materialized recovery directory changed before installation"
                );
                install_staged_directory(&store.path, &staging, "pre-restore")
            })();
            if staging.exists() {
                cleanup_created_directory(parent, &staging);
            }
            result
        }
    }
}

fn install_reset(destination: &Path, candidate: &ResetCandidate, reason: &str) -> Result<()> {
    match candidate {
        ResetCandidate::File(bytes) => install_file_with(destination, bytes, reason, save_atomic)
            .context("atomically install reset file"),
        ResetCandidate::EmptyDirectory => install_directory(destination, None, reason),
    }
}

fn install_file_with(
    destination: &Path,
    bytes: &[u8],
    reason: &str,
    writer: impl FnOnce(&Path, &[u8]) -> Result<()>,
) -> Result<()> {
    backup_file(destination, reason).context("preserve current destination")?;
    writer(destination, bytes)
}

fn install_directory(destination: &Path, source: Option<&Path>, reason: &str) -> Result<()> {
    let parent = destination
        .parent()
        .context("directory destination has no parent")?;
    fs::create_dir_all(parent).context("create recovery destination parent")?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("store");
    let staging = reserve_sibling(parent, &format!(".{name}.recovery-staging"))?;
    let build = match source {
        Some(source) => copy_tree(source, &staging),
        None => Ok(()),
    };
    if let Err(error) = build {
        cleanup_created_directory(parent, &staging);
        return Err(error);
    }
    let result = install_staged_directory(destination, &staging, reason);
    if staging.exists() {
        cleanup_created_directory(parent, &staging);
    }
    result
}

fn install_staged_directory(destination: &Path, staging: &Path, reason: &str) -> Result<()> {
    let parent = destination
        .parent()
        .context("directory destination has no parent")?;
    ensure_safe_staging_directory(parent, staging)?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("store");
    let backup = if destination.exists() {
        Some(reserve_absent_sibling(parent, &format!("{name}.{reason}"))?)
    } else {
        None
    };
    if let Some(backup) = &backup {
        if let Err(error) = fs::rename(destination, backup) {
            cleanup_created_directory(parent, &staging);
            return Err(error).context("preserve current recovery directory");
        }
    }
    if let Err(error) = fs::rename(&staging, destination) {
        if let Some(backup) = &backup {
            if let Err(rollback_error) = fs::rename(backup, destination) {
                cleanup_created_directory(parent, &staging);
                bail!(
                    "install recovery directory failed ({error}); rollback failed ({rollback_error}); preserved destination is {}",
                    backup.display()
                );
            }
        }
        cleanup_created_directory(parent, &staging);
        return Err(error).context("install complete recovery directory");
    }
    Ok(())
}

fn ensure_safe_staging_directory(parent: &Path, staging: &Path) -> Result<()> {
    let safe_name = staging.file_name().is_some_and(|name| {
        name.to_string_lossy().starts_with('.')
            && name.to_string_lossy().contains(".recovery-staging.")
    });
    ensure!(safe_name, "unrecognized recovery staging directory");
    reject_tree_reparse(staging)?;
    let canonical_parent = fs::canonicalize(parent).context("resolve recovery parent")?;
    let canonical_staging = fs::canonicalize(staging).context("resolve recovery staging tree")?;
    ensure!(
        canonical_staging.parent() == Some(canonical_parent.as_path()),
        "recovery staging directory escapes destination parent"
    );
    Ok(())
}

fn cleanup_created_file(parent: &Path, path: &Path) {
    let safe_name = path.file_name().is_some_and(|name| {
        name.to_string_lossy().starts_with('.')
            && name.to_string_lossy().contains(".recovery-staging.")
    });
    let contained = fs::canonicalize(parent).is_ok_and(|canonical_parent| {
        fs::canonicalize(path)
            .is_ok_and(|canonical_path| canonical_path.parent() == Some(canonical_parent.as_path()))
    });
    if safe_name && contained && reject_reparse(path).is_ok() {
        let _ = fs::remove_file(path);
    }
}

fn cleanup_created_directory(parent: &Path, path: &Path) {
    let safe_name = path.file_name().is_some_and(|name| {
        name.to_string_lossy().starts_with('.')
            && name.to_string_lossy().contains(".recovery-staging.")
    });
    let contained = fs::canonicalize(parent).is_ok_and(|canonical_parent| {
        fs::canonicalize(path)
            .is_ok_and(|canonical_path| canonical_path.parent() == Some(canonical_parent.as_path()))
    });
    if safe_name && contained && reject_tree_reparse(path).is_ok() {
        let _ = fs::remove_dir_all(path);
    }
}

fn canonical_reset(id: PersistentStoreId, kind: StoreKind) -> Result<ResetCandidate> {
    if kind == StoreKind::Directory {
        return match id {
            PersistentStoreId::Notes | PersistentStoreId::NoteTemplates => {
                Ok(ResetCandidate::EmptyDirectory)
            }
            _ => bail!("store has no canonical directory reset form"),
        };
    }
    macro_rules! json {
        ($value:expr) => {{ ResetCandidate::File(serde_json::to_vec_pretty(&$value)?) }};
    }
    use PersistentStoreId as Id;
    Ok(match id {
        Id::Settings => json!(Settings::default()),
        Id::Actions => json!(Vec::<Action>::new()),
        Id::Bookmarks => json!(Vec::<BookmarkEntry>::new()),
        Id::Folders => json!(default_folders()),
        Id::Snippets => json!(Vec::<SnippetEntry>::new()),
        Id::Favorites => json!(Vec::<FavEntry>::new()),
        Id::Todos => json!(Vec::<TodoEntry>::new()),
        Id::ShellCommands => json!(Vec::<ShellCmdEntry>::new()),
        Id::LegacyMacros => json!(Vec::<MacroEntry>::new()),
        Id::HistoryPins => json!(Vec::<HistoryPin>::new()),
        Id::CalendarEvents => json!(Vec::<CalendarEvent>::new()),
        Id::Layouts => json!(LayoutStore::default()),
        Id::DashboardConfig => json!(DashboardConfig::default()),
        Id::MouseGestureDefinitions => json!(GestureDb::default()),
        Id::MkMacroDocument => json!(MkMacroDocument::default()),
        Id::ClipboardModifiers => json!(crate::clipboard_modify::config::default_model()),
        Id::MultiManagerWorkspaces => json!(Vec::<MmWorkspace>::new()),
        Id::Scratchpad => json!(serde_json::json!({ "content": "" })),
        Id::QueryHistory => json!(Vec::<HistoryEntry>::new()),
        Id::ClipboardHistory => json!(Vec::<String>::new()),
        Id::CalculatorHistory => json!(Vec::<CalcHistoryEntry>::new()),
        Id::Usage => json!(Vec::<UsageEntry>::new()),
        Id::CalendarState => json!(CalendarState::default()),
        Id::MouseGestureUsage => json!(Vec::<GestureUsageEntry>::new()),
        Id::MouseGestureState => {
            json!(crate::mouse_gestures::service::GestureSelectionState::default())
        }
        Id::NoteUiState => json!(NoteUiState::default()),
        Id::MultiManagerBindings => json!(Vec::<WorkspaceBindingSnapshot>::new()),
        Id::Alarms => json!(Vec::<serde_json::Value>::new()),
        _ => bail!("store has no canonical reset form"),
    })
}

fn validate_owned_destination(root: &AppDataRoot, destination: &Path) -> Result<()> {
    ensure!(
        !destination
            .components()
            .any(|c| matches!(c, Component::ParentDir)),
        "destination contains traversal"
    );
    let canonical_root = fs::canonicalize(root.path()).context("resolve application data root")?;
    ensure!(
        destination.starts_with(root.path()),
        "destination escapes application data root"
    );
    reject_reparse(&canonical_root)?;
    let relative = destination
        .strip_prefix(root.path())
        .context("destination escapes root")?;
    let mut current = canonical_root;
    for component in relative.components() {
        ensure!(
            matches!(component, Component::Normal(_)),
            "unsafe destination component"
        );
        current.push(component.as_os_str());
        if current.exists() {
            reject_reparse(&current)?;
        }
    }
    Ok(())
}

fn validated_pending_path(root: &AppDataRoot, create: bool) -> Result<PathBuf> {
    if create {
        fs::create_dir_all(root.path()).context("create application data root")?;
    }
    let canonical_root = fs::canonicalize(root.path()).context("resolve application data root")?;
    reject_reparse(root.path())?;
    let recovery = root.path().join(RECOVERY_DIRECTORY);
    if !recovery.exists() {
        ensure!(create, "recovery directory does not exist");
        fs::create_dir(&recovery).context("create recovery directory")?;
    }
    reject_reparse(&recovery)?;
    let canonical_recovery = fs::canonicalize(&recovery).context("resolve recovery directory")?;
    ensure!(
        canonical_recovery.starts_with(&canonical_root),
        "recovery directory escapes application data root"
    );
    let pending = recovery.join(PENDING_FILE);
    if pending.exists() {
        reject_reparse(&pending)?;
    }
    Ok(pending)
}

fn validate_snapshot_candidate(root: &Path, candidate: &Path, kind: StoreKind) -> Result<()> {
    ensure!(
        candidate.starts_with(root),
        "snapshot candidate escapes snapshot root"
    );
    let canonical_root = fs::canonicalize(root).context("resolve snapshot root")?;
    let canonical = fs::canonicalize(candidate).context("resolve snapshot candidate")?;
    ensure!(
        canonical.starts_with(&canonical_root),
        "snapshot candidate escapes snapshot root"
    );
    reject_tree_reparse(candidate)?;
    let metadata = fs::metadata(candidate)?;
    ensure!(
        matches!(
            (kind, metadata.is_file(), metadata.is_dir()),
            (StoreKind::File, true, _) | (StoreKind::Directory, _, true)
        ),
        "snapshot candidate kind does not match catalog"
    );
    Ok(())
}

fn fingerprint(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    reject_reparse_metadata(path, &metadata)?;
    let mut hash = Fnv64::default();
    if metadata.is_file() {
        hash.update(&[b'F']);
        hash_file_framed(path, &mut hash)?;
    } else if metadata.is_dir() {
        hash.update(&[b'D']);
        for relative in regular_tree_paths(path)? {
            hash.update(&[b'E']);
            hash_segment(&mut hash, relative.to_string_lossy().as_bytes());
            hash_file_framed(&path.join(relative), &mut hash)?;
        }
    } else {
        bail!("unsupported recovery candidate kind");
    }
    Ok(format!("fnv1a64:{:016x}", hash.0))
}

fn fingerprint_bytes(kind: u8, bytes: &[u8]) -> String {
    let mut hash = Fnv64::default();
    hash.update(&[kind]);
    hash_segment(&mut hash, bytes);
    format!("fnv1a64:{:016x}", hash.0)
}

fn hash_json(value: &serde_json::Value, hash: &mut Fnv64) {
    match value {
        serde_json::Value::Null => hash.update(b"n"),
        serde_json::Value::Bool(value) => hash.update(if *value { b"t" } else { b"f" }),
        serde_json::Value::Number(value) => {
            hash.update(b"#");
            hash_segment(hash, value.to_string().as_bytes());
        }
        serde_json::Value::String(value) => {
            hash.update(b"s");
            hash_segment(hash, value.as_bytes());
        }
        serde_json::Value::Array(values) => {
            hash.update(b"[");
            hash.update(&(values.len() as u64).to_le_bytes());
            for value in values {
                hash_json(value, hash);
            }
        }
        serde_json::Value::Object(values) => {
            hash.update(b"{");
            hash.update(&(values.len() as u64).to_le_bytes());
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for key in keys {
                hash_segment(hash, key.as_bytes());
                hash_json(&values[key], hash);
            }
        }
    }
}

fn hash_segment(hash: &mut Fnv64, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

struct Fnv64(u64);
impl Default for Fnv64 {
    fn default() -> Self {
        Self(0xcbf29ce484222325)
    }
}
impl Fnv64 {
    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

fn hash_file_framed(path: &Path, hash: &mut Fnv64) -> Result<()> {
    let mut file = fs::File::open(path)?;
    hash.update(&file.metadata()?.len().to_le_bytes());
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(())
}

fn regular_tree_paths(root: &Path) -> Result<Vec<PathBuf>> {
    fn visit(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        reject_reparse(current)?;
        let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            reject_reparse_metadata(&path, &metadata)?;
            if metadata.is_dir() {
                visit(root, &path, files)?;
            } else if metadata.is_file() {
                files.push(path.strip_prefix(root)?.to_path_buf());
            } else {
                bail!("unsupported entry in recovery candidate");
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fn visit(source: &Path, destination: &Path) -> Result<()> {
        reject_reparse(source)?;
        fs::create_dir_all(destination)?;
        let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let source_path = entry.path();
            let metadata = fs::symlink_metadata(&source_path)?;
            reject_reparse_metadata(&source_path, &metadata)?;
            let destination_path = destination.join(entry.file_name());
            if metadata.is_dir() {
                visit(&source_path, &destination_path)?;
            } else if metadata.is_file() {
                fs::copy(source_path, destination_path)?;
            } else {
                bail!("unsupported entry in recovery candidate");
            }
        }
        Ok(())
    }
    visit(source, destination)
}

fn reject_tree_reparse(path: &Path) -> Result<()> {
    reject_reparse(path)?;
    if fs::metadata(path)?.is_dir() {
        for entry in fs::read_dir(path)? {
            reject_tree_reparse(&entry?.path())?;
        }
    }
    Ok(())
}

fn reject_reparse(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
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
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
#[cfg(not(windows))]
fn is_reparse(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn reserve_sibling(parent: &Path, prefix: &str) -> Result<PathBuf> {
    for index in 0..1000 {
        let path = parent.join(format!("{prefix}.{}-{index}", timestamp()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("unable to reserve recovery staging directory")
}

fn reserve_staging_file(parent: &Path, prefix: &str, bytes: &[u8]) -> Result<PathBuf> {
    for index in 0..1000 {
        let path = parent.join(format!("{prefix}.{}-{index}", timestamp()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let result = (|| -> std::io::Result<()> {
                    file.write_all(bytes)?;
                    file.flush()?;
                    file.sync_all()
                })();
                drop(file);
                if let Err(error) = result {
                    cleanup_created_file(parent, &path);
                    return Err(error).context("materialize recovery staging file");
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("reserve recovery staging file"),
        }
    }
    bail!("unable to reserve recovery staging file")
}

fn reserve_absent_sibling(parent: &Path, prefix: &str) -> Result<PathBuf> {
    for index in 0..1000 {
        let path = parent.join(format!("{prefix}.{}-{index}.bak", timestamp()));
        if !path.exists() {
            return Ok(path);
        }
    }
    bail!("unable to reserve recovery backup path")
}

fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn validate_snapshot_id(id: &str) -> Result<()> {
    ensure!(!id.is_empty() && id.len() <= 120, "invalid snapshot ID");
    ensure!(
        id.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "invalid snapshot ID"
    );
    Ok(())
}

fn safe_relative(path: &Path) -> bool {
    !path.is_absolute() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .replace('/', "\\")
            .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, AppDataRoot, PersistenceCatalog) {
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(dir.path().to_path_buf());
        fs::write(
            dir.path().join("settings.json"),
            serde_json::to_vec_pretty(&Settings::default()).unwrap(),
        )
        .unwrap();
        let catalog = PersistenceCatalog::new(&root, &Settings::default());
        (dir, root, catalog)
    }

    fn snapshot_id(root: &AppDataRoot, catalog: &PersistenceCatalog) -> String {
        BackupEngine::new(root, catalog)
            .create_snapshot()
            .unwrap()
            .manifest
            .snapshot_id
    }

    #[test]
    fn stage_request_is_atomic_and_read_only_for_destination() {
        let (_dir, root, catalog) = fixture();
        let before = fs::read(root.path().join("settings.json")).unwrap();
        let id = snapshot_id(&root, &catalog);
        let pending = RecoveryManager::new(&root, &catalog)
            .stage_restore(PersistentStoreId::Settings, &id)
            .unwrap();
        assert_eq!(fs::read(root.path().join("settings.json")).unwrap(), before);
        assert_eq!(
            serde_json::from_slice::<PendingRecoveryDescriptor>(
                &fs::read(root.path().join("recovery/pending.json")).unwrap()
            )
            .unwrap(),
            pending
        );
        assert!(
            !root
                .path()
                .join("recovery")
                .read_dir()
                .unwrap()
                .any(|entry| entry.unwrap().file_name().to_string_lossy().contains("tmp"))
        );
    }

    #[test]
    fn invalid_snapshot_id_is_rejected_without_pending_or_destination_change() {
        let (_dir, root, catalog) = fixture();
        let before = fs::read(root.path().join("settings.json")).unwrap();
        assert!(
            RecoveryManager::new(&root, &catalog)
                .stage_restore(PersistentStoreId::Settings, "../escape")
                .is_err()
        );
        assert_eq!(fs::read(root.path().join("settings.json")).unwrap(), before);
        assert!(!root.path().join("recovery/pending.json").exists());
    }

    #[test]
    fn changed_snapshot_bytes_leave_destination_and_pending_actionable() {
        let (_dir, root, catalog) = fixture();
        let id = snapshot_id(&root, &catalog);
        RecoveryManager::new(&root, &catalog)
            .stage_restore(PersistentStoreId::Settings, &id)
            .unwrap();
        let destination = root.path().join("settings.json");
        fs::write(&destination, b"live bytes").unwrap();
        fs::write(
            root.path()
                .join("backups")
                .join(&id)
                .join("stores/Settings"),
            serde_json::to_vec(&Settings::default()).unwrap(),
        )
        .unwrap();
        let result = apply_pending_recovery(&root);
        assert!(matches!(
            result.diagnostic,
            Some(RecoveryStartupDiagnostic::ApplyFailed { .. })
        ));
        assert_eq!(fs::read(&destination).unwrap(), b"live bytes");
        assert!(root.path().join("recovery/pending.json").exists());
    }

    #[test]
    fn source_mutation_after_materialization_cannot_change_installed_file() {
        let (_dir, root, catalog) = fixture();
        let id = snapshot_id(&root, &catalog);
        let manager = RecoveryManager::new(&root, &catalog);
        let pending = manager
            .stage_restore(PersistentStoreId::Settings, &id)
            .unwrap();
        let store = catalog.get(PersistentStoreId::Settings);
        let candidate = manager.restore_candidate(store, &id).unwrap();
        let materialized_bytes = fs::read(&candidate.path).unwrap();
        fs::write(&store.path, b"current live bytes").unwrap();

        apply_restore_with_hook(store, &candidate, &pending.candidate_fingerprint, &|| {
            let mut changed = Settings::default();
            changed.show_examples = !changed.show_examples;
            fs::write(
                &candidate.path,
                serde_json::to_vec_pretty(&changed).unwrap(),
            )
            .unwrap();
        })
        .unwrap();

        assert_ne!(fs::read(&candidate.path).unwrap(), materialized_bytes);
        assert_eq!(fs::read(&store.path).unwrap(), materialized_bytes);
    }

    #[test]
    fn source_mutation_after_materialization_cannot_change_installed_directory() {
        let (dir, _root, catalog) = fixture();
        let source = dir.path().join("snapshot-notes");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("note.md"), b"validated note").unwrap();
        let candidate = Candidate {
            path: source.clone(),
            kind: StoreKind::Directory,
        };
        let expected_fingerprint = fingerprint(&source).unwrap();
        let mut store = catalog.get(PersistentStoreId::Notes).clone();
        store.path = dir.path().join("live-notes");
        fs::create_dir(&store.path).unwrap();
        fs::write(store.path.join("old.md"), b"old live note").unwrap();

        apply_restore_with_hook(&store, &candidate, &expected_fingerprint, &|| {
            fs::write(source.join("note.md"), b"changed snapshot note").unwrap();
        })
        .unwrap();

        assert_eq!(
            fs::read(source.join("note.md")).unwrap(),
            b"changed snapshot note"
        );
        assert_eq!(
            fs::read(store.path.join("note.md")).unwrap(),
            b"validated note"
        );
    }

    #[test]
    fn successful_restore_preserves_destination_and_clears_only_after_success() {
        let (_dir, root, catalog) = fixture();
        let original = fs::read(root.path().join("settings.json")).unwrap();
        let id = snapshot_id(&root, &catalog);
        RecoveryManager::new(&root, &catalog)
            .stage_restore(PersistentStoreId::Settings, &id)
            .unwrap();
        fs::write(root.path().join("settings.json"), b"corrupt current").unwrap();
        let result = apply_pending_recovery(&root);
        assert!(result.diagnostic.is_none(), "{:?}", result.diagnostic);
        assert_eq!(
            fs::read(root.path().join("settings.json")).unwrap(),
            original
        );
        assert!(!root.path().join("recovery/pending.json").exists());
        assert!(fs::read_dir(root.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("settings.json.pre-restore.")
        }));
    }

    #[test]
    fn invalid_pending_is_retained_with_typed_diagnostic() {
        let (_dir, root, _catalog) = fixture();
        fs::create_dir(root.path().join("recovery")).unwrap();
        fs::write(root.path().join("recovery/pending.json"), b"{bad").unwrap();
        let result = apply_pending_recovery(&root);
        assert!(matches!(
            result.diagnostic,
            Some(RecoveryStartupDiagnostic::InvalidPending { .. })
        ));
        assert!(root.path().join("recovery/pending.json").exists());
    }

    #[test]
    fn corrupt_reset_preserves_original_and_installs_canonical_form() {
        let (_dir, root, catalog) = fixture();
        let destination = root.path().join("settings.json");
        fs::write(&destination, b"corrupt settings").unwrap();
        RecoveryManager::new(&root, &catalog)
            .stage_reset(PersistentStoreId::Settings)
            .unwrap();
        let result = apply_pending_recovery(&root);
        assert!(result.diagnostic.is_none(), "{:?}", result.diagnostic);
        assert!(serde_json::from_slice::<Settings>(&fs::read(&destination).unwrap()).is_ok());
        let backup = fs::read_dir(root.path())
            .unwrap()
            .map(Result::unwrap)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.json.pre-reset.")
            })
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), b"corrupt settings");
    }

    #[test]
    fn failed_domain_validation_is_retryable_and_preserves_live_bytes() {
        let (_dir, root, catalog) = fixture();
        let id = snapshot_id(&root, &catalog);
        RecoveryManager::new(&root, &catalog)
            .stage_restore(PersistentStoreId::Settings, &id)
            .unwrap();
        let pending_path = root.path().join("recovery/pending.json");
        let mut pending: PendingRecoveryDescriptor =
            serde_json::from_slice(&fs::read(&pending_path).unwrap()).unwrap();
        let source = root
            .path()
            .join("backups")
            .join(&id)
            .join("stores/Settings");
        fs::write(&source, b"not settings").unwrap();
        pending.candidate_fingerprint = fingerprint(&source).unwrap();
        save_atomic(&pending_path, &serde_json::to_vec_pretty(&pending).unwrap()).unwrap();
        fs::write(root.path().join("settings.json"), b"current").unwrap();
        let result = apply_pending_recovery(&root);
        assert!(result.diagnostic.is_some());
        assert_eq!(
            fs::read(root.path().join("settings.json")).unwrap(),
            b"current"
        );
        assert!(pending_path.exists());
    }

    #[test]
    fn directory_replacement_preserves_complete_previous_tree() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("notes");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/new.md"), "new").unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old.md"), "old").unwrap();
        install_directory(&destination, Some(&source), "pre-restore").unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("nested/new.md")).unwrap(),
            "new"
        );
        let backup = fs::read_dir(dir.path())
            .unwrap()
            .map(Result::unwrap)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("notes.pre-restore.")
            })
            .unwrap();
        assert_eq!(fs::read_to_string(backup.join("old.md")).unwrap(), "old");
    }

    #[test]
    fn failed_file_install_preserves_destination_and_creates_reasoned_backup() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("settings.json");
        fs::write(&destination, b"current").unwrap();

        let result = install_file_with(&destination, b"replacement", "pre-restore", |_, _| {
            bail!("injected write failure")
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"current");
        let backup = fs::read_dir(dir.path())
            .unwrap()
            .map(Result::unwrap)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.json.pre-restore.")
            })
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), b"current");
    }

    #[test]
    fn every_reset_candidate_passes_its_domain_probe() {
        let (dir, _root, catalog) = fixture();
        for id in PersistentStoreId::ALL {
            if matches!(
                id,
                PersistentStoreId::MkMacroAssets
                    | PersistentStoreId::NotesAssets
                    | PersistentStoreId::LauncherLog
                    | PersistentStoreId::ToastLog
            ) {
                continue;
            }
            let kind = if matches!(
                id,
                PersistentStoreId::Notes | PersistentStoreId::NoteTemplates
            ) {
                StoreKind::Directory
            } else {
                StoreKind::File
            };
            let candidate = canonical_reset(id, kind).unwrap();
            let path = dir.path().join(format!("candidate-{id:?}"));
            match candidate {
                ResetCandidate::File(bytes) => fs::write(&path, bytes).unwrap(),
                ResetCandidate::EmptyDirectory => fs::create_dir(&path).unwrap(),
            }
            assert!(
                matches!(
                    catalog.get(id).probe_path(&path),
                    StoreHealth::Healthy | StoreHealth::Empty
                ),
                "reset candidate for {id:?} failed its probe"
            );
            match id {
                PersistentStoreId::Folders => assert_eq!(
                    crate::plugins::folders::load_folders(path.to_str().unwrap()).unwrap(),
                    default_folders()
                ),
                PersistentStoreId::MouseGestureState => assert_eq!(
                    crate::mouse_gestures::service::load_selection_state(path.to_str().unwrap()),
                    crate::mouse_gestures::service::GestureSelectionState::default()
                ),
                _ => {}
            }
        }
    }
}
