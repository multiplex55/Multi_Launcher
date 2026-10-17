//! Confirmed search-and-replace writes and durable restore receipts.
//!
//! Candidate bytes and selected identities are frozen before this module can
//! touch disk. Native source guards and no-replace moves remain in
//! `file_operations`; this module owns operation admission, backup receipts,
//! progress, and truthful per-file outcomes.

use crate::batch_rename::{self, ExplicitDirectoryRoot, SnapshotEntryKind};
use crate::common::persistence::save_json_atomic;
use crate::file_operations::{
    self, GuardedReplaceFile, GuardedReplaceStage, MAX_RENAME_HASH_BYTES_PER_BATCH,
    MAX_RENAME_HASH_BYTES_PER_FILE, NativeFileIdentity, RenameScanGeneration, RenameScanRequest,
    RenameScanStatus, ScanCancellation, VerifiedReadSnapshot, VerifiedReplaceFailureKind,
};
use crate::search_replace::{
    MatchId, ReplacementCandidate, ReplacementCandidateReadiness, ReplacementFileReadiness,
    ReplacementRevision, SearchGeneration, SearchRevision,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

pub const REPLACE_RECOVERY_DIRECTORY: &str = "replace_recovery";
pub const REPLACE_RECOVERY_JOURNAL_FILE: &str = "search_replace_recovery.json";
pub const MAX_RETAINED_REPLACE_RECEIPTS: usize = 24;
pub const MAX_REPLACE_FILES_PER_BATCH: usize = 128;
const MAX_REPLACE_JOURNAL_BYTES: usize = 16 * 1024 * 1024;
const REPLACE_JOURNAL_SCHEMA_VERSION: u32 = 1;
const MAX_MESSAGE_CHARS: usize = 2_048;

static RESTORE_SCAN_GENERATION: AtomicU64 = AtomicU64::new(1);

mod system_time_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct EncodedSystemTime {
        before_unix_epoch: bool,
        seconds: u64,
        nanoseconds: u32,
    }

    pub fn serialize<S>(value: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let (before_unix_epoch, duration) = match value.duration_since(UNIX_EPOCH) {
            Ok(duration) => (false, duration),
            Err(error) => (true, error.duration()),
        };
        EncodedSystemTime {
            before_unix_epoch,
            seconds: duration.as_secs(),
            nanoseconds: duration.subsec_nanos(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        decode(EncodedSystemTime::deserialize(deserializer)?)
    }

    pub(super) fn decode<E: Error>(value: EncodedSystemTime) -> Result<SystemTime, E> {
        if value.nanoseconds >= 1_000_000_000 {
            return Err(E::custom("invalid SystemTime nanoseconds"));
        }
        let duration = Duration::new(value.seconds, value.nanoseconds);
        if value.before_unix_epoch {
            UNIX_EPOCH
                .checked_sub(duration)
                .ok_or_else(|| E::custom("SystemTime is outside supported range"))
        } else {
            UNIX_EPOCH
                .checked_add(duration)
                .ok_or_else(|| E::custom("SystemTime is outside supported range"))
        }
    }
}

/// Explicit approval boundary for one immutable B candidate. This consumes
/// and copies the exact selected byte payloads; it never recomputes preview.
#[derive(Debug, Clone)]
pub struct ConfirmedReplacementRequest {
    search_revision: SearchRevision,
    search_generation: SearchGeneration,
    replacement_revision: ReplacementRevision,
    selected_match_ids: Vec<MatchId>,
    files: Vec<FrozenReplacementFile>,
}

#[derive(Debug, Clone)]
struct FrozenReplacementFile {
    root_path: PathBuf,
    root_identity: NativeFileIdentity,
    relative_path: PathBuf,
    path: PathBuf,
    identity: NativeFileIdentity,
    parent_identity: NativeFileIdentity,
    original_length: u64,
    original_modified: SystemTime,
    original_bytes: std::sync::Arc<[u8]>,
    original_sha256: [u8; 32],
    output_bytes: std::sync::Arc<[u8]>,
    output_sha256: [u8; 32],
    evidence: std::sync::Arc<file_operations::VerifiedReadEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmReplacementError {
    message: String,
}

impl ConfirmReplacementError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ConfirmReplacementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConfirmReplacementError {}

impl ConfirmedReplacementRequest {
    pub fn confirm(candidate: ReplacementCandidate) -> Result<Self, ConfirmReplacementError> {
        if !candidate.discovery_complete()
            || candidate.readiness() != ReplacementCandidateReadiness::Ready
            || candidate.selected_match_ids().is_empty()
        {
            return Err(ConfirmReplacementError::new(
                "replacement requires a complete, ready candidate with an explicit selection",
            ));
        }
        let mut selected = HashSet::new();
        if candidate
            .selected_match_ids()
            .iter()
            .any(|match_id| !selected.insert(match_id.clone()))
        {
            return Err(ConfirmReplacementError::new(
                "the approved replacement selection contains duplicate match identities",
            ));
        }

        let mut files = Vec::new();
        let mut selected_from_files = HashSet::new();
        let mut total_bytes = 0_u64;
        for file in candidate.files() {
            if file.selected_match_ids().iter().any(|match_id| {
                !selected.contains(match_id) || !selected_from_files.insert(match_id.clone())
            }) {
                return Err(ConfirmReplacementError::new(
                    "per-file selected match identities do not exactly belong to the approved selection",
                ));
            }
            match file.readiness() {
                ReplacementFileReadiness::Error => {
                    return Err(ConfirmReplacementError::new(
                        "a per-file replacement candidate is incomplete or has an error",
                    ));
                }
                ReplacementFileReadiness::NoOp => continue,
                ReplacementFileReadiness::Ready => {}
            }
            let snapshot = file.original_snapshot();
            let original = snapshot.raw_bytes().as_ref();
            let output = file.output_bytes().as_ref();
            if file.selected_match_ids().is_empty()
                || original.len() as u64 != snapshot.length()
                || <[u8; 32]>::from(Sha256::digest(original)) != snapshot.content_sha256()
                || output == original
                || <[u8; 32]>::from(Sha256::digest(output)) != file.output_sha256()
                || file.replacement_revision() != candidate.replacement_revision()
                || output.len() as u64 > MAX_RENAME_HASH_BYTES_PER_FILE
            {
                return Err(ConfirmReplacementError::new(
                    "a selected replacement file does not have complete immutable source and output bytes",
                ));
            }
            let evidence = snapshot.source_evidence().cloned().ok_or_else(|| {
                ConfirmReplacementError::new(
                    "the selected source lacks opaque native scan evidence for safe mutation",
                )
            })?;
            if !snapshot.root().is_absolute()
                || !snapshot.path().is_absolute()
                || !is_confined_relative_path(snapshot.relative_path())
                || batch_rename::compare_paths_windows_ordinal(
                    &snapshot.root().join(snapshot.relative_path()),
                    snapshot.path(),
                ) != std::cmp::Ordering::Equal
            {
                return Err(ConfirmReplacementError::new(
                    "a selected replacement path is not a confined absolute source under its recorded root",
                ));
            }
            if snapshot.root().to_str().is_none()
                || snapshot.relative_path().to_str().is_none()
                || snapshot.path().to_str().is_none()
            {
                return Err(ConfirmReplacementError::new(
                    "replacement recovery requires Unicode root and relative paths",
                ));
            }
            total_bytes = total_bytes
                .checked_add(snapshot.length())
                .and_then(|sum| sum.checked_add(output.len() as u64))
                .ok_or_else(|| {
                    ConfirmReplacementError::new("replacement batch byte count overflowed")
                })?;
            if total_bytes > MAX_RENAME_HASH_BYTES_PER_BATCH {
                return Err(ConfirmReplacementError::new(
                    "the selected replacement batch exceeds the bounded write and recovery limit",
                ));
            }
            if files.len() >= MAX_REPLACE_FILES_PER_BATCH {
                return Err(ConfirmReplacementError::new(
                    "the selected replacement exceeds the file-count limit",
                ));
            }
            files.push(FrozenReplacementFile {
                root_path: snapshot.root().to_path_buf(),
                root_identity: snapshot.root_identity(),
                relative_path: snapshot.relative_path().to_path_buf(),
                path: snapshot.path().to_path_buf(),
                identity: snapshot.file_identity(),
                parent_identity: snapshot.parent_identity(),
                original_length: snapshot.length(),
                original_modified: snapshot.modified(),
                original_bytes: std::sync::Arc::clone(snapshot.raw_bytes()),
                original_sha256: snapshot.content_sha256(),
                output_bytes: std::sync::Arc::clone(file.output_bytes()),
                output_sha256: file.output_sha256(),
                evidence,
            });
        }
        if selected_from_files != selected {
            return Err(ConfirmReplacementError::new(
                "the approved match selection does not exactly match the immutable changed-file outputs",
            ));
        }
        if files.is_empty() || files.len() > MAX_REPLACE_FILES_PER_BATCH {
            return Err(ConfirmReplacementError::new(
                "the confirmed replacement candidate has no changed files or exceeds the file-count limit",
            ));
        }
        Ok(Self {
            search_revision: candidate.search_revision(),
            search_generation: candidate.search_generation(),
            replacement_revision: candidate.replacement_revision(),
            selected_match_ids: candidate.selected_match_ids().to_vec(),
            files,
        })
    }
    pub fn changed_file_count(&self) -> usize {
        self.files.len()
    }

    pub fn approved_match_count(&self) -> usize {
        self.selected_match_ids.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplaceTransactionStatus {
    Applying,
    Applied,
    Partial,
    Restoring,
    Restored,
    RecoveryRequired,
    Failed,
    Cancelled,
    Refused,
}

impl ReplaceTransactionStatus {
    fn blocks_new_apply(self) -> bool {
        matches!(
            self,
            Self::Applying | Self::Restoring | Self::RecoveryRequired
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplaceEntryOutcome {
    Pending,
    Applied,
    Restored,
    StaleSkipped,
    Refused,
    Failed,
    Cancelled,
    NotAttempted,
    RecoveryRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReplaceEntryPhase {
    Planned,
    BackupPending,
    BackupReady,
    OutputStagePending,
    OutputStageReady,
    OriginalMovePending,
    OriginalMoved,
    PublishPending,
    CleanupPending,
    Applied,
    RestorePending,
    RestoreStageReady,
    RestoreSourceMovePending,
    RestoreSourceMoved,
    RestorePublishPending,
    Restored,
    Refused,
    StaleSkipped,
    Failed,
    Cancelled,
    NotAttempted,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceRecoveryJournal {
    schema_version: u32,
    receipts: Vec<ReplaceRecoveryReceipt>,
}

impl Default for ReplaceRecoveryJournal {
    fn default() -> Self {
        Self {
            schema_version: REPLACE_JOURNAL_SCHEMA_VERSION,
            receipts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceRecoveryReceipt {
    id: String,
    search_revision: u64,
    search_generation: u64,
    replacement_revision: u64,
    status: ReplaceTransactionStatus,
    entries: Vec<ReplaceRecoveryEntry>,
    message: String,
}

impl ReplaceRecoveryReceipt {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn status(&self) -> ReplaceTransactionStatus {
        self.status
    }
    pub fn entries(&self) -> &[ReplaceRecoveryEntry] {
        &self.entries
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceRecoveryEntry {
    root_path: PathBuf,
    root_identity: NativeFileIdentity,
    relative_path: PathBuf,
    parent_identity: NativeFileIdentity,
    original_identity: NativeFileIdentity,
    original_length: u64,
    #[serde(with = "system_time_serde")]
    original_modified: SystemTime,
    original_sha256: [u8; 32],
    output_length: u64,
    output_sha256: [u8; 32],
    backup_leaf: String,
    backup_identity: Option<NativeFileIdentity>,
    output_stage_leaf: String,
    output_stage_identity: Option<NativeFileIdentity>,
    original_stage_leaf: String,
    #[serde(default)]
    original_stage_identity: Option<NativeFileIdentity>,
    post_identity: Option<NativeFileIdentity>,
    #[serde(default, with = "option_system_time_serde")]
    post_modified: Option<SystemTime>,
    restore_stage_leaf: String,
    restore_stage_identity: Option<NativeFileIdentity>,
    restore_source_stage_leaf: String,
    #[serde(default)]
    restore_source_stage_identity: Option<NativeFileIdentity>,
    restored_identity: Option<NativeFileIdentity>,
    #[serde(default, with = "option_system_time_serde")]
    restored_modified: Option<SystemTime>,
    phase: ReplaceEntryPhase,
    outcome: ReplaceEntryOutcome,
    message: String,
}

impl ReplaceRecoveryEntry {
    pub fn path(&self) -> PathBuf {
        self.root_path.join(&self.relative_path)
    }
    pub fn outcome(&self) -> ReplaceEntryOutcome {
        self.outcome
    }
    pub fn original_sha256(&self) -> [u8; 32] {
        self.original_sha256
    }
    pub fn output_sha256(&self) -> [u8; 32] {
        self.output_sha256
    }
}

mod option_system_time_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::SystemTime;

    pub fn serialize<S>(value: &Option<SystemTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(time) => super::system_time_serde::serialize(time, serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<SystemTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<super::system_time_serde::EncodedSystemTime>::deserialize(deserializer)?
            .map(super::system_time_serde::decode)
            .transpose()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceProgress {
    pub transaction_id: String,
    pub phase: ReplaceTransactionStatus,
    pub completed: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ReplaceOperationResult {
    pub receipt: ReplaceRecoveryReceipt,
    pub message: String,
}

pub struct ReplaceExecutor {
    recovery_directory: PathBuf,
    operation_lock: Mutex<()>,
}

impl ReplaceExecutor {
    pub fn new(recovery_directory: impl Into<PathBuf>) -> Result<Self, String> {
        let recovery_directory = recovery_directory.into();
        if !recovery_directory.is_absolute()
            || recovery_directory
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err("replacement recovery directory must be an absolute confined path".into());
        }
        if recovery_directory.to_str().is_none() {
            return Err("replacement recovery directory must be Unicode".into());
        }
        Ok(Self {
            recovery_directory,
            operation_lock: Mutex::new(()),
        })
    }

    fn operation_guard(
        &self,
        transaction_id: &str,
    ) -> Result<std::sync::MutexGuard<'_, ()>, ReplaceOperationResult> {
        self.operation_lock.try_lock().map_err(|error| match error {
            std::sync::TryLockError::WouldBlock => refused_result(
                transaction_id,
                "another replace operation is already active".into(),
            ),
            std::sync::TryLockError::Poisoned(_) => refused_result(
                transaction_id,
                "replace operation state is unavailable".into(),
            ),
        })
    }

    pub fn receipts(&self) -> Result<Vec<ReplaceRecoveryReceipt>, String> {
        load_replace_journal(&self.journal_path()).map(|journal| journal.receipts)
    }
    fn journal_path(&self) -> PathBuf {
        self.recovery_directory.join(REPLACE_RECOVERY_JOURNAL_FILE)
    }

    pub fn execute_confirmed(
        &self,
        request: ConfirmedReplacementRequest,
        cancellation: &ScanCancellation,
        mut report: impl FnMut(ReplaceProgress),
    ) -> ReplaceOperationResult {
        let _operation = match self.operation_guard("") {
            Ok(guard) => guard,
            Err(result) => return result,
        };
        if cancellation.is_cancelled() {
            return refused_result("", "replacement was cancelled before recovery setup".into());
        }
        #[cfg(not(windows))]
        return refused_result(
            "",
            "safe replacement writes are available only on Windows".into(),
        );
        #[cfg(windows)]
        self.execute_confirmed_windows(request, cancellation, &mut report)
    }

    #[cfg(windows)]
    fn execute_confirmed_windows(
        &self,
        request: ConfirmedReplacementRequest,
        cancellation: &ScanCancellation,
        report: &mut impl FnMut(ReplaceProgress),
    ) -> ReplaceOperationResult {
        self.execute_confirmed_windows_with_publication_boundary(
            request,
            cancellation,
            report,
            &mut |_| {},
        )
    }

    #[cfg(windows)]
    fn execute_confirmed_windows_with_publication_boundary(
        &self,
        request: ConfirmedReplacementRequest,
        cancellation: &ScanCancellation,
        report: &mut impl FnMut(ReplaceProgress),
        before_output_publication: &mut impl FnMut(&Path),
    ) -> ReplaceOperationResult {
        let transaction_id = format!("{:032x}", rand::random::<u128>());
        let recovery_directory = match ensure_recovery_directory(&self.recovery_directory) {
            Ok(path) => path,
            Err(error) => return refused_result(&transaction_id, error),
        };
        let journal_path = recovery_directory.join(REPLACE_RECOVERY_JOURNAL_FILE);
        let mut journal = match load_replace_journal(&journal_path) {
            Ok(journal) => journal,
            Err(error) => return refused_result(&transaction_id, error),
        };
        if journal
            .receipts
            .iter()
            .any(|receipt| receipt.status.blocks_new_apply())
        {
            return refused_result(
                &transaction_id,
                "an unresolved replacement receipt must be reconciled before another apply".into(),
            );
        }
        if journal.receipts.len() >= MAX_RETAINED_REPLACE_RECEIPTS {
            return refused_result(
                &transaction_id,
                "the replacement recovery journal is full; retained receipts must be reviewed before applying another batch".into(),
            );
        }
        let mut receipt = receipt_from_request(&transaction_id, &request);
        journal.receipts.push(receipt.clone());
        if let Err(error) = save_replace_journal(&journal_path, &journal) {
            return failed_receipt(
                receipt,
                format!("recovery intent could not be saved before any source changed: {error}"),
            );
        }
        report(ReplaceProgress {
            transaction_id: transaction_id.clone(),
            phase: ReplaceTransactionStatus::Applying,
            completed: 0,
            total: request.files.len(),
            message: "Rechecking and replacing the explicitly approved files".into(),
        });

        for index in 0..request.files.len() {
            if cancellation.is_cancelled() {
                mark_remaining(
                    &mut receipt,
                    index,
                    ReplaceEntryOutcome::Cancelled,
                    ReplaceEntryPhase::Cancelled,
                    "cancelled before this file was attempted",
                );
                let _ = update_receipt(&mut journal, &journal_path, &receipt);
                receipt.status = if receipt
                    .entries
                    .iter()
                    .any(|entry| entry.outcome == ReplaceEntryOutcome::Applied)
                {
                    ReplaceTransactionStatus::Partial
                } else {
                    ReplaceTransactionStatus::Cancelled
                };
                receipt.message =
                    "replacement was cancelled between files; completed files remain committed"
                        .into();
                let _ = update_receipt(&mut journal, &journal_path, &receipt);
                return ReplaceOperationResult {
                    message: receipt.message.clone(),
                    receipt,
                };
            }
            report(ReplaceProgress {
                transaction_id: transaction_id.clone(),
                phase: ReplaceTransactionStatus::Applying,
                completed: index,
                total: request.files.len(),
                message: format!("Rechecking {}", request.files[index].path.display()),
            });
            match apply_one_file(
                &request.files[index],
                index,
                &mut receipt,
                &mut journal,
                &journal_path,
                &recovery_directory,
                cancellation,
                before_output_publication,
            ) {
                ApplyOneResult::Applied => {}
                ApplyOneResult::Cancelled {
                    message,
                    recovery_required,
                } => {
                    receipt.entries[index].outcome = if recovery_required {
                        ReplaceEntryOutcome::RecoveryRequired
                    } else {
                        ReplaceEntryOutcome::Cancelled
                    };
                    receipt.entries[index].phase = if recovery_required {
                        ReplaceEntryPhase::RecoveryRequired
                    } else {
                        ReplaceEntryPhase::Cancelled
                    };
                    receipt.entries[index].message = bounded_message(&message);
                    mark_remaining(
                        &mut receipt,
                        index + 1,
                        ReplaceEntryOutcome::NotAttempted,
                        ReplaceEntryPhase::NotAttempted,
                        "not attempted after cancellation or recovery stop",
                    );
                    receipt.status = if recovery_required {
                        ReplaceTransactionStatus::RecoveryRequired
                    } else if receipt
                        .entries
                        .iter()
                        .any(|entry| entry.outcome == ReplaceEntryOutcome::Applied)
                    {
                        ReplaceTransactionStatus::Partial
                    } else {
                        ReplaceTransactionStatus::Cancelled
                    };
                    receipt.message = bounded_message(&message);
                    let _ = update_receipt(&mut journal, &journal_path, &receipt);
                    return ReplaceOperationResult {
                        message: receipt.message.clone(),
                        receipt,
                    };
                }
                ApplyOneResult::Skip {
                    root_changed,
                    message,
                } => {
                    receipt.entries[index].outcome = ReplaceEntryOutcome::StaleSkipped;
                    receipt.entries[index].phase = ReplaceEntryPhase::StaleSkipped;
                    receipt.entries[index].message = bounded_message(&message);
                    if let Err(error) = update_receipt(&mut journal, &journal_path, &receipt) {
                        receipt.entries[index].outcome = ReplaceEntryOutcome::RecoveryRequired;
                        receipt.entries[index].phase = ReplaceEntryPhase::RecoveryRequired;
                        receipt.status = ReplaceTransactionStatus::RecoveryRequired;
                        receipt.message =
                            format!("stale per-file result could not be saved: {error}");
                        return ReplaceOperationResult {
                            message: receipt.message.clone(),
                            receipt,
                        };
                    }
                    if root_changed {
                        mark_remaining(
                            &mut receipt,
                            index + 1,
                            ReplaceEntryOutcome::NotAttempted,
                            ReplaceEntryPhase::NotAttempted,
                            "not attempted because the selected root identity changed",
                        );
                        break;
                    }
                }
                ApplyOneResult::Refused {
                    message,
                    recovery_required,
                } => {
                    receipt.entries[index].outcome = if recovery_required {
                        ReplaceEntryOutcome::RecoveryRequired
                    } else {
                        ReplaceEntryOutcome::Refused
                    };
                    receipt.entries[index].phase = if recovery_required {
                        ReplaceEntryPhase::RecoveryRequired
                    } else {
                        ReplaceEntryPhase::Refused
                    };
                    receipt.entries[index].message = bounded_message(&message);
                    if recovery_required {
                        receipt.status = ReplaceTransactionStatus::RecoveryRequired;
                        receipt.message = bounded_message(&message);
                        let _ = update_receipt(&mut journal, &journal_path, &receipt);
                        mark_remaining(
                            &mut receipt,
                            index + 1,
                            ReplaceEntryOutcome::NotAttempted,
                            ReplaceEntryPhase::NotAttempted,
                            "not attempted because an earlier file needs recovery",
                        );
                        let _ = update_receipt(&mut journal, &journal_path, &receipt);
                        return ReplaceOperationResult {
                            message: receipt.message.clone(),
                            receipt,
                        };
                    }
                    if let Err(error) = update_receipt(&mut journal, &journal_path, &receipt) {
                        receipt.status = ReplaceTransactionStatus::RecoveryRequired;
                        receipt.message = format!("per-file refusal could not be saved: {error}");
                        return ReplaceOperationResult {
                            message: receipt.message.clone(),
                            receipt,
                        };
                    }
                }
            }
        }
        let any_applied = receipt
            .entries
            .iter()
            .any(|entry| entry.outcome == ReplaceEntryOutcome::Applied);
        let any_unattempted = receipt.entries.iter().any(|entry| {
            matches!(
                entry.outcome,
                ReplaceEntryOutcome::NotAttempted
                    | ReplaceEntryOutcome::Cancelled
                    | ReplaceEntryOutcome::StaleSkipped
                    | ReplaceEntryOutcome::Refused
            )
        });
        receipt.status = if any_applied && any_unattempted {
            ReplaceTransactionStatus::Partial
        } else if any_applied {
            ReplaceTransactionStatus::Applied
        } else if receipt
            .entries
            .iter()
            .any(|entry| entry.outcome == ReplaceEntryOutcome::RecoveryRequired)
        {
            ReplaceTransactionStatus::RecoveryRequired
        } else if cancellation.is_cancelled() {
            ReplaceTransactionStatus::Cancelled
        } else {
            ReplaceTransactionStatus::Failed
        };
        receipt.message = match receipt.status {
            ReplaceTransactionStatus::Applied => {
                "all selected changed files were replaced and verified".into()
            }
            ReplaceTransactionStatus::Partial => {
                "replacement completed partially; each file outcome is recorded".into()
            }
            ReplaceTransactionStatus::Cancelled => {
                "replacement ended without committing a file".into()
            }
            ReplaceTransactionStatus::RecoveryRequired => {
                "replacement stopped with a retained recovery payload".into()
            }
            _ => "no selected file was replaced".into(),
        };
        if let Err(error) = update_receipt(&mut journal, &journal_path, &receipt) {
            receipt.status = ReplaceTransactionStatus::RecoveryRequired;
            receipt.message = format!("final replacement receipt could not be saved: {error}");
        }
        ReplaceOperationResult {
            message: receipt.message.clone(),
            receipt,
        }
    }

    pub fn restore(
        &self,
        transaction_id: &str,
        cancellation: &ScanCancellation,
        mut report: impl FnMut(ReplaceProgress),
    ) -> ReplaceOperationResult {
        let _operation = match self.operation_guard(transaction_id) {
            Ok(guard) => guard,
            Err(result) => return result,
        };
        if cancellation.is_cancelled() {
            return refused_result(
                transaction_id,
                "Restore was cancelled before recovery setup".into(),
            );
        }
        #[cfg(not(windows))]
        return refused_result(
            transaction_id,
            "safe replacement Restore is available only on Windows".into(),
        );
        #[cfg(windows)]
        self.restore_windows(transaction_id, cancellation, &mut report)
    }

    /// Reconcile one interrupted transaction using only recorded role paths,
    /// identities, hashes, and the same guarded no-replace native moves.
    pub fn recover(
        &self,
        transaction_id: &str,
        cancellation: &ScanCancellation,
        mut report: impl FnMut(ReplaceProgress),
    ) -> ReplaceOperationResult {
        let _operation = match self.operation_guard(transaction_id) {
            Ok(guard) => guard,
            Err(result) => return result,
        };
        if cancellation.is_cancelled() {
            return refused_result(
                transaction_id,
                "recovery was cancelled before journal reconciliation".into(),
            );
        }
        #[cfg(not(windows))]
        return refused_result(
            transaction_id,
            "safe replacement recovery is available only on Windows".into(),
        );
        #[cfg(windows)]
        self.recover_windows(transaction_id, cancellation, &mut report)
    }

    #[cfg(windows)]
    fn restore_windows(
        &self,
        transaction_id: &str,
        cancellation: &ScanCancellation,
        report: &mut impl FnMut(ReplaceProgress),
    ) -> ReplaceOperationResult {
        let recovery_directory = match ensure_recovery_directory(&self.recovery_directory) {
            Ok(path) => path,
            Err(error) => return refused_result(transaction_id, error),
        };
        let journal_path = recovery_directory.join(REPLACE_RECOVERY_JOURNAL_FILE);
        let mut journal = match load_replace_journal(&journal_path) {
            Ok(journal) => journal,
            Err(error) => return refused_result(transaction_id, error),
        };
        let Some(receipt_index) = journal
            .receipts
            .iter()
            .position(|receipt| receipt.id == transaction_id)
        else {
            return refused_result(
                transaction_id,
                "replacement recovery receipt was not found".into(),
            );
        };
        let mut receipt = journal.receipts[receipt_index].clone();
        if receipt.status == ReplaceTransactionStatus::Restored {
            for entry in &receipt.entries {
                if entry.outcome == ReplaceEntryOutcome::Restored {
                    if let Err(message) = verify_already_restored(entry, cancellation) {
                        return refused_receipt(
                            receipt.clone(),
                            format!("repeated Restore refused: {message}"),
                        );
                    }
                }
            }
            return ReplaceOperationResult {
                message: "this replacement transaction was already restored; no files changed"
                    .into(),
                receipt,
            };
        }
        if !matches!(
            receipt.status,
            ReplaceTransactionStatus::Applied | ReplaceTransactionStatus::Partial
        ) {
            return refused_receipt(
                receipt,
                "this replacement receipt is unresolved or has no safely restorable committed file",
            );
        }
        if cancellation.is_cancelled() {
            return refused_receipt(receipt, "Restore was cancelled before any file changed");
        }

        if !receipt.entries.iter().any(|entry| {
            matches!(
                entry.outcome,
                ReplaceEntryOutcome::Applied | ReplaceEntryOutcome::Refused
            ) && entry.post_identity.is_some()
                && entry.backup_identity.is_some()
        }) {
            return refused_receipt(
                receipt,
                "this receipt has no verified committed output and original backup to restore",
            );
        }
        let prior_status = receipt.status;
        receipt.status = ReplaceTransactionStatus::Restoring;
        receipt.message = "guarded Restore is in progress".into();
        journal.receipts[receipt_index] = receipt.clone();
        if let Err(error) = save_replace_journal(&journal_path, &journal) {
            return refused_receipt(
                receipt,
                format!("Restore was not started because its intent could not be saved: {error}"),
            );
        }

        let total = receipt
            .entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.outcome,
                    ReplaceEntryOutcome::Applied
                        | ReplaceEntryOutcome::Refused
                        | ReplaceEntryOutcome::Restored
                )
            })
            .count();
        let mut completed = 0;
        for entry_index in 0..receipt.entries.len() {
            if receipt.entries[entry_index].outcome == ReplaceEntryOutcome::Restored {
                if let Err(message) =
                    verify_already_restored(&receipt.entries[entry_index], cancellation)
                {
                    receipt.status = ReplaceTransactionStatus::RecoveryRequired;
                    receipt.message =
                        bounded_message(&format!("repeated Restore refused: {message}"));
                    journal.receipts[receipt_index] = receipt.clone();
                    let _ = save_replace_journal(&journal_path, &journal);
                    return ReplaceOperationResult {
                        message: receipt.message.clone(),
                        receipt,
                    };
                }
                completed += 1;
                continue;
            }
            if !matches!(
                receipt.entries[entry_index].outcome,
                ReplaceEntryOutcome::Applied | ReplaceEntryOutcome::Refused
            ) || receipt.entries[entry_index].post_identity.is_none()
                || receipt.entries[entry_index].backup_identity.is_none()
            {
                continue;
            }
            if cancellation.is_cancelled() {
                receipt.status = if completed > 0 {
                    ReplaceTransactionStatus::Partial
                } else {
                    prior_status
                };
                receipt.message =
                    "Restore was cancelled between files; completed restores remain committed"
                        .into();
                journal.receipts[receipt_index] = receipt.clone();
                if let Err(error) = save_replace_journal(&journal_path, &journal) {
                    receipt.status = ReplaceTransactionStatus::RecoveryRequired;
                    receipt.message = format!(
                        "Restore was cancelled and its receipt could not be saved: {error}"
                    );
                }
                return ReplaceOperationResult {
                    message: receipt.message.clone(),
                    receipt,
                };
            }
            report(ReplaceProgress {
                transaction_id: receipt.id.clone(),
                phase: ReplaceTransactionStatus::Restoring,
                completed,
                total,
                message: format!(
                    "Verifying {} before Restore",
                    receipt.entries[entry_index].path().display()
                ),
            });
            match restore_one_entry(
                entry_index,
                &mut receipt,
                &mut journal,
                receipt_index,
                &journal_path,
                &recovery_directory,
                cancellation,
            ) {
                RestoreOneResult::Restored => completed += 1,
                RestoreOneResult::Cancelled {
                    message,
                    recovery_required,
                } => {
                    receipt.status = if recovery_required {
                        ReplaceTransactionStatus::RecoveryRequired
                    } else if completed > 0 {
                        ReplaceTransactionStatus::Partial
                    } else {
                        prior_status
                    };
                    receipt.message = bounded_message(&message);
                    mark_remaining(
                        &mut receipt,
                        entry_index + 1,
                        ReplaceEntryOutcome::NotAttempted,
                        ReplaceEntryPhase::NotAttempted,
                        "not attempted after Restore cancellation",
                    );
                    journal.receipts[receipt_index] = receipt.clone();
                    let saved = save_replace_journal(&journal_path, &journal);
                    if saved.is_err() {
                        receipt.status = ReplaceTransactionStatus::RecoveryRequired;
                        receipt.message = format!(
                            "{}; receipt save failed: {}",
                            receipt.message,
                            saved.unwrap_err()
                        );
                    }
                    return ReplaceOperationResult {
                        message: receipt.message.clone(),
                        receipt,
                    };
                }
                RestoreOneResult::Refused {
                    message,
                    recovery_required,
                    committed,
                } => {
                    let entry = &mut receipt.entries[entry_index];
                    if recovery_required {
                        entry.phase = ReplaceEntryPhase::RecoveryRequired;
                    } else if committed {
                        entry.phase = ReplaceEntryPhase::Restored;
                    }
                    entry.outcome = if committed {
                        ReplaceEntryOutcome::Restored
                    } else if recovery_required {
                        ReplaceEntryOutcome::RecoveryRequired
                    } else {
                        ReplaceEntryOutcome::Refused
                    };
                    entry.message = bounded_message(&message);
                    receipt.status = if recovery_required {
                        ReplaceTransactionStatus::RecoveryRequired
                    } else if committed {
                        ReplaceTransactionStatus::RecoveryRequired
                    } else if completed > 0 {
                        ReplaceTransactionStatus::Partial
                    } else {
                        prior_status
                    };
                    receipt.message = bounded_message(&message);
                    journal.receipts[receipt_index] = receipt.clone();
                    if let Err(error) = save_replace_journal(&journal_path, &journal) {
                        receipt.status = ReplaceTransactionStatus::RecoveryRequired;
                        receipt.message = bounded_message(&format!(
                            "{}; Restore outcome could not be saved: {error}",
                            receipt.message
                        ));
                    }
                    return ReplaceOperationResult {
                        message: receipt.message.clone(),
                        receipt,
                    };
                }
            }
        }
        let committed = receipt
            .entries
            .iter()
            .filter(|entry| entry.post_identity.is_some() && entry.backup_identity.is_some())
            .count();
        let remaining = receipt
            .entries
            .iter()
            .filter(|entry| {
                entry.post_identity.is_some() && entry.outcome != ReplaceEntryOutcome::Restored
            })
            .count();
        receipt.status = if committed > 0 && remaining == 0 {
            ReplaceTransactionStatus::Restored
        } else {
            ReplaceTransactionStatus::Partial
        };
        receipt.message = if receipt.status == ReplaceTransactionStatus::Restored {
            "all committed files were restored from verified original backups".into()
        } else {
            "Restore completed partially; remaining file outcomes are recorded".into()
        };
        journal.receipts[receipt_index] = receipt.clone();
        if let Err(error) = save_replace_journal(&journal_path, &journal) {
            receipt.status = ReplaceTransactionStatus::RecoveryRequired;
            receipt.message = format!(
                "restored file state is verified but its receipt could not be saved: {error}"
            );
        }
        ReplaceOperationResult {
            message: receipt.message.clone(),
            receipt,
        }
    }

    #[cfg(windows)]
    fn recover_windows(
        &self,
        transaction_id: &str,
        cancellation: &ScanCancellation,
        report: &mut impl FnMut(ReplaceProgress),
    ) -> ReplaceOperationResult {
        let recovery_directory = match ensure_recovery_directory(&self.recovery_directory) {
            Ok(path) => path,
            Err(error) => return refused_result(transaction_id, error),
        };
        let journal_path = recovery_directory.join(REPLACE_RECOVERY_JOURNAL_FILE);
        let mut journal = match load_replace_journal(&journal_path) {
            Ok(journal) => journal,
            Err(error) => return refused_result(transaction_id, error),
        };
        let Some(receipt_index) = journal
            .receipts
            .iter()
            .position(|receipt| receipt.id == transaction_id)
        else {
            return refused_result(
                transaction_id,
                "replacement recovery receipt was not found".into(),
            );
        };
        let mut receipt = journal.receipts[receipt_index].clone();
        if !receipt.status.blocks_new_apply() {
            return ReplaceOperationResult {
                message: "this replacement transaction has no interrupted operation to recover"
                    .into(),
                receipt,
            };
        }
        let restoring = receipt.status == ReplaceTransactionStatus::Restoring
            || receipt.entries.iter().any(|entry| {
                entry.restore_stage_identity.is_some()
                    || entry.restore_source_stage_identity.is_some()
                    || entry.restored_identity.is_some()
                    || matches!(
                        entry.phase,
                        ReplaceEntryPhase::RestorePending
                            | ReplaceEntryPhase::RestoreStageReady
                            | ReplaceEntryPhase::RestoreSourceMovePending
                            | ReplaceEntryPhase::RestoreSourceMoved
                            | ReplaceEntryPhase::RestorePublishPending
                            | ReplaceEntryPhase::Restored
                    )
            });
        let total = receipt.entries.len();
        let mut unresolved = false;
        for entry_index in 0..receipt.entries.len() {
            if cancellation.is_cancelled() {
                receipt.status = ReplaceTransactionStatus::RecoveryRequired;
                receipt.message =
                    "recovery was cancelled between guarded entry reconciliations".into();
                journal.receipts[receipt_index] = receipt.clone();
                let _ = save_replace_journal(&journal_path, &journal);
                return ReplaceOperationResult {
                    message: receipt.message.clone(),
                    receipt,
                };
            }
            let entry = &receipt.entries[entry_index];
            let should_reconcile = if restoring {
                entry.outcome == ReplaceEntryOutcome::RecoveryRequired
                    || entry.phase == ReplaceEntryPhase::RecoveryRequired
                    || matches!(
                        entry.phase,
                        ReplaceEntryPhase::RestorePending
                            | ReplaceEntryPhase::RestoreStageReady
                            | ReplaceEntryPhase::RestoreSourceMovePending
                            | ReplaceEntryPhase::RestoreSourceMoved
                            | ReplaceEntryPhase::RestorePublishPending
                            | ReplaceEntryPhase::Restored
                    )
            } else {
                entry.outcome == ReplaceEntryOutcome::RecoveryRequired
                    || entry.phase == ReplaceEntryPhase::RecoveryRequired
                    || (receipt.status == ReplaceTransactionStatus::Applying
                        && entry.phase == ReplaceEntryPhase::Planned)
                    || matches!(
                        entry.phase,
                        ReplaceEntryPhase::BackupPending
                            | ReplaceEntryPhase::BackupReady
                            | ReplaceEntryPhase::OutputStagePending
                            | ReplaceEntryPhase::OutputStageReady
                            | ReplaceEntryPhase::OriginalMovePending
                            | ReplaceEntryPhase::OriginalMoved
                            | ReplaceEntryPhase::PublishPending
                            | ReplaceEntryPhase::CleanupPending
                    )
            };
            if !should_reconcile {
                continue;
            }
            report(ReplaceProgress {
                transaction_id: receipt.id.clone(),
                phase: ReplaceTransactionStatus::RecoveryRequired,
                completed: entry_index,
                total,
                message: format!(
                    "Rechecking recorded recovery roles for {}",
                    entry.path().display()
                ),
            });
            let result = if restoring {
                recover_interrupted_restore(
                    entry_index,
                    &mut receipt,
                    &mut journal,
                    receipt_index,
                    &journal_path,
                    &recovery_directory,
                    cancellation,
                )
            } else {
                recover_interrupted_apply(
                    entry_index,
                    &mut receipt,
                    &mut journal,
                    receipt_index,
                    &journal_path,
                    &recovery_directory,
                    cancellation,
                )
            };
            if let Err(message) = result {
                let mut entry = receipt.entries[entry_index].clone();
                entry.phase = ReplaceEntryPhase::RecoveryRequired;
                entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
                entry.message = bounded_message(&message);
                let _ = persist_entry(
                    &mut journal,
                    &journal_path,
                    &mut receipt,
                    entry_index,
                    &entry,
                );
                unresolved = true;
                receipt.message = bounded_message(&message);
                break;
            }
        }

        if unresolved {
            receipt.status = ReplaceTransactionStatus::RecoveryRequired;
        } else if restoring {
            let committed = receipt
                .entries
                .iter()
                .filter(|entry| entry.post_identity.is_some() && entry.backup_identity.is_some())
                .count();
            let restored = receipt
                .entries
                .iter()
                .filter(|entry| entry.outcome == ReplaceEntryOutcome::Restored)
                .count();
            receipt.status = if committed > 0 && restored == committed {
                ReplaceTransactionStatus::Restored
            } else if restored > 0 {
                ReplaceTransactionStatus::Partial
            } else if committed > 0 {
                ReplaceTransactionStatus::Applied
            } else {
                ReplaceTransactionStatus::Failed
            };
        } else {
            let any_applied = receipt
                .entries
                .iter()
                .any(|entry| entry.outcome == ReplaceEntryOutcome::Applied);
            let any_other = receipt.entries.iter().any(|entry| {
                matches!(
                    entry.outcome,
                    ReplaceEntryOutcome::NotAttempted
                        | ReplaceEntryOutcome::Cancelled
                        | ReplaceEntryOutcome::StaleSkipped
                        | ReplaceEntryOutcome::Refused
                        | ReplaceEntryOutcome::Failed
                )
            });
            receipt.status = if any_applied && any_other {
                ReplaceTransactionStatus::Partial
            } else if any_applied {
                ReplaceTransactionStatus::Applied
            } else {
                ReplaceTransactionStatus::Failed
            };
        }
        if !unresolved {
            receipt.message = if restoring {
                "interrupted Restore phases were reconciled from recorded identities and hashes"
                    .into()
            } else {
                "interrupted apply phases were reconciled from recorded identities and hashes"
                    .into()
            };
        }
        journal.receipts[receipt_index] = receipt.clone();
        if let Err(error) = save_replace_journal(&journal_path, &journal) {
            receipt.status = ReplaceTransactionStatus::RecoveryRequired;
            receipt.message = bounded_message(&format!(
                "reconciled file state could not be durably recorded: {error}"
            ));
        }
        ReplaceOperationResult {
            message: receipt.message.clone(),
            receipt,
        }
    }
}

#[derive(Debug)]
enum ApplyOneResult {
    Applied,
    Skip {
        root_changed: bool,
        message: String,
    },
    Cancelled {
        message: String,
        recovery_required: bool,
    },
    Refused {
        message: String,
        recovery_required: bool,
    },
}

fn receipt_from_request(
    transaction_id: &str,
    request: &ConfirmedReplacementRequest,
) -> ReplaceRecoveryReceipt {
    let entries = request
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| ReplaceRecoveryEntry {
            root_path: file.root_path.clone(),
            root_identity: file.root_identity,
            relative_path: file.relative_path.clone(),
            parent_identity: file.parent_identity,
            original_identity: file.identity,
            original_length: file.original_length,
            original_modified: file.original_modified,
            original_sha256: file.original_sha256,
            output_length: file.output_bytes.len() as u64,
            output_sha256: file.output_sha256,
            backup_leaf: role_leaf(transaction_id, index, "backup"),
            backup_identity: None,
            output_stage_leaf: role_leaf(transaction_id, index, "output"),
            output_stage_identity: None,
            original_stage_leaf: role_leaf(transaction_id, index, "source"),
            original_stage_identity: None,
            post_identity: None,
            post_modified: None,
            restore_stage_leaf: role_leaf(transaction_id, index, "restore"),
            restore_stage_identity: None,
            restore_source_stage_leaf: role_leaf(transaction_id, index, "post"),
            restore_source_stage_identity: None,
            restored_identity: None,
            restored_modified: None,
            phase: ReplaceEntryPhase::Planned,
            outcome: ReplaceEntryOutcome::Pending,
            message: String::new(),
        })
        .collect();
    ReplaceRecoveryReceipt {
        id: transaction_id.to_owned(),
        search_revision: request.search_revision.0,
        search_generation: request.search_generation.0,
        replacement_revision: request.replacement_revision.0,
        status: ReplaceTransactionStatus::Applying,
        entries,
        message: "replacement intent recorded before source changes".into(),
    }
}

#[cfg(windows)]
fn apply_one_file(
    file: &FrozenReplacementFile,
    entry_index: usize,
    receipt: &mut ReplaceRecoveryReceipt,
    journal: &mut ReplaceRecoveryJournal,
    journal_path: &Path,
    recovery_directory: &Path,
    cancellation: &ScanCancellation,
    before_output_publication: &mut impl FnMut(&Path),
) -> ApplyOneResult {
    let mut entry = receipt.entries[entry_index].clone();
    let Some(original_leaf) = file.path.file_name() else {
        return ApplyOneResult::Refused {
            message: "the confirmed source no longer has a file name".into(),
            recovery_required: false,
        };
    };
    let mut source = match file_operations::prepare_guarded_replace_file(
        &file.evidence,
        &file.root_path,
        file.root_identity,
        &file.path,
        file.identity,
        file.parent_identity,
        file.original_length,
        file.original_modified,
        file.original_sha256,
        &file.original_bytes,
        cancellation,
    ) {
        Ok(source) => source,
        Err(error) => {
            let root_changed = error.kind() == VerifiedReplaceFailureKind::RootChanged;
            return if matches!(
                error.kind(),
                VerifiedReplaceFailureKind::RootChanged
                    | VerifiedReplaceFailureKind::ParentChanged
                    | VerifiedReplaceFailureKind::SourceChanged
            ) {
                ApplyOneResult::Skip {
                    root_changed,
                    message: error.message().to_owned(),
                }
            } else {
                ApplyOneResult::Refused {
                    message: error.message().to_owned(),
                    recovery_required: false,
                }
            };
        }
    };
    if cancellation.is_cancelled() {
        return ApplyOneResult::Cancelled {
            message: "replacement cancelled before creating a backup".into(),
            recovery_required: false,
        };
    }

    entry.phase = ReplaceEntryPhase::BackupPending;
    entry.message = "writing and verifying the original recovery payload".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        return ApplyOneResult::Refused {
            message: format!(
                "original backup was not created because its intent could not be saved: {error}"
            ),
            recovery_required: true,
        };
    }

    let backup_path = recovery_directory.join(&entry.backup_leaf);
    let backup = match file_operations::create_verified_replace_backup(
        &backup_path,
        &file.original_bytes,
        file.original_sha256,
        cancellation,
    ) {
        Ok(backup) => backup,
        Err(error) => {
            let collision = error.kind() == std::io::ErrorKind::AlreadyExists;
            return ApplyOneResult::Refused {
                message: format!("verified original backup could not be created: {error}"),
                recovery_required: !collision,
            };
        }
    };
    entry.backup_identity = Some(backup.state().identity);
    entry.phase = ReplaceEntryPhase::BackupReady;
    entry.message = "verified original backup is durable".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        return ApplyOneResult::Refused {
            message: format!(
                "original backup is durable at {} but its verified receipt could not be saved: {error}",
                backup.path().display()
            ),
            recovery_required: true,
        };
    }

    if cancellation.is_cancelled() {
        entry.outcome = ReplaceEntryOutcome::Cancelled;
        entry.phase = ReplaceEntryPhase::Cancelled;
        entry.message = "cancelled after durable backup and before output staging".into();
        return match persist_entry(journal, journal_path, receipt, entry_index, &entry) {
            Ok(()) => ApplyOneResult::Cancelled {
                message: entry.message.clone(),
                recovery_required: false,
            },
            Err(error) => ApplyOneResult::Cancelled {
                message: format!(
                    "cancelled before source mutation, but the outcome could not be saved: {error}"
                ),
                recovery_required: true,
            },
        };
    }

    entry.phase = ReplaceEntryPhase::OutputStagePending;
    entry.message = "creating a CREATE_NEW sibling stage for exact approved output".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        return ApplyOneResult::Refused {
            message: format!(
                "output stage was not created because its intent could not be saved: {error}"
            ),
            recovery_required: true,
        };
    }
    let output_leaf = std::ffi::OsStr::new(&entry.output_stage_leaf);
    let mut output = match source.create_output_stage(
        output_leaf,
        &file.output_bytes,
        file.output_sha256,
        None,
        cancellation,
    ) {
        Ok(stage) => stage,
        Err(error) => {
            let retained = error.to_string().contains("stage remains");
            let collision = error.kind() == std::io::ErrorKind::AlreadyExists;
            return ApplyOneResult::Refused {
                message: format!("exact output could not be staged: {error}"),
                recovery_required: retained
                    || (!collision && matches!(error.kind(), std::io::ErrorKind::Other)),
            };
        }
    };
    entry.output_stage_identity = Some(output.state().identity);
    entry.phase = ReplaceEntryPhase::OutputStageReady;
    entry.message = "exact output stage was flushed and verified".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        let cleanup = output.delete_if_matches(&ScanCancellation::new());
        return ApplyOneResult::Refused {
            message: format!(
                "verified output stage could not be recorded before source mutation: {error}; stage cleanup: {cleanup:?}"
            ),
            recovery_required: cleanup.is_err(),
        };
    }

    if cancellation.is_cancelled() {
        let cleanup = output.delete_if_matches(&ScanCancellation::new());
        entry.outcome = if cleanup.is_ok() {
            ReplaceEntryOutcome::Cancelled
        } else {
            ReplaceEntryOutcome::RecoveryRequired
        };
        entry.phase = if cleanup.is_ok() {
            ReplaceEntryPhase::Cancelled
        } else {
            ReplaceEntryPhase::RecoveryRequired
        };
        entry.message = if cleanup.is_ok() {
            "cancelled before source relocation; verified output stage was removed".into()
        } else {
            format!(
                "cancelled before source relocation; output stage remains at {} ({cleanup:?})",
                output.path().display()
            )
        };
        let saved = persist_entry(journal, journal_path, receipt, entry_index, &entry);
        let (message, save_failed) = match saved {
            Ok(()) => (entry.message.clone(), false),
            Err(error) => (
                format!("{}; outcome save failed: {error}", entry.message),
                true,
            ),
        };
        return ApplyOneResult::Cancelled {
            message,
            recovery_required: cleanup.is_err() || save_failed,
        };
    }

    entry.phase = ReplaceEntryPhase::OriginalMovePending;
    entry.message =
        "durable backup and output stage are ready; relocating the verified original".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        let cleanup = output.delete_if_matches(&ScanCancellation::new());
        return ApplyOneResult::Refused {
            message: format!(
                "source was not moved because its relocation intent could not be saved: {error}; output stage cleanup: {cleanup:?}"
            ),
            recovery_required: cleanup.is_err(),
        };
    }
    let original_state = source.original_state();
    if let Err(error) = source.rename_to_leaf(
        std::ffi::OsStr::new(&entry.original_stage_leaf),
        original_state,
        cancellation,
    ) {
        if batch_rename::compare_paths_windows_ordinal(source.current_path(), &file.path)
            != std::cmp::Ordering::Equal
        {
            entry.original_stage_identity = Some(original_state.identity);
            entry.phase = ReplaceEntryPhase::OriginalMoved;
            entry.message = format!("original moved but post-move verification failed: {error}");
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return ApplyOneResult::Refused {
                message: entry.message.clone(),
                recovery_required: true,
            };
        }
        let cleanup = output.delete_if_matches(&ScanCancellation::new());
        return ApplyOneResult::Refused {
            message: format!("original was not moved: {error}; output stage cleanup: {cleanup:?}"),
            recovery_required: cleanup.is_err(),
        };
    }
    entry.original_stage_identity = Some(original_state.identity);
    entry.phase = ReplaceEntryPhase::OriginalMoved;
    entry.message = "verified original is held in its owned sibling stage".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        let compensated = compensate_original(&mut source, original_state, original_leaf);
        return ApplyOneResult::Refused {
            message: format!(
                "source relocation completed but could not be journaled: {error}; guarded compensation: {compensated:?}"
            ),
            recovery_required: compensated.is_err(),
        };
    }

    before_output_publication(&file.path);
    if cancellation.is_cancelled() {
        return cancel_between_moves(
            &mut source,
            &output,
            original_state,
            original_leaf,
            entry_index,
            &mut entry,
            journal,
            journal_path,
            receipt,
        );
    }
    entry.phase = ReplaceEntryPhase::PublishPending;
    entry.message = "publishing the verified output stage without replacing any occupant".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        return compensate_before_publish(
            &mut source,
            &output,
            original_state,
            original_leaf,
            entry_index,
            &mut entry,
            journal,
            journal_path,
            receipt,
            format!("publication intent could not be saved: {error}"),
        );
    }
    if let Err(error) = output.rename_to_leaf(original_leaf, cancellation) {
        if batch_rename::compare_paths_windows_ordinal(output.path(), &file.path)
            == std::cmp::Ordering::Equal
        {
            entry.post_identity = Some(output.state().identity);
            entry.post_modified = Some(output.state().modified);
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.message =
                format!("output moved but verification at the original path failed: {error}");
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return ApplyOneResult::Refused {
                message: entry.message.clone(),
                recovery_required: true,
            };
        }
        return compensate_before_publish(
            &mut source,
            &output,
            original_state,
            original_leaf,
            entry_index,
            &mut entry,
            journal,
            journal_path,
            receipt,
            format!("output publication did not complete: {error}"),
        );
    }
    let post = match output.verify_at(&file.path, &ScanCancellation::new()) {
        Ok(state) => state,
        Err(error) => {
            entry.post_identity = Some(output.state().identity);
            entry.post_modified = Some(output.state().modified);
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.message =
                format!("output is at the selected path but final verification failed: {error}");
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return ApplyOneResult::Refused {
                message: entry.message.clone(),
                recovery_required: true,
            };
        }
    };
    entry.post_identity = Some(post.identity);
    entry.post_modified = Some(post.modified);
    entry.phase = ReplaceEntryPhase::Applied;
    entry.outcome = ReplaceEntryOutcome::Applied;
    entry.message =
        "approved output bytes were published and verified; original backup retained".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        return ApplyOneResult::Refused {
            message: format!(
                "published output is verified, but its commit receipt could not be saved: {error}"
            ),
            recovery_required: true,
        };
    }

    // The durable terminal receipt and appdata backup now cover the original;
    // cleanup is by the still-open, identity-pinned source handle only.
    if let Err(error) = source.delete_if_matches(original_state, &ScanCancellation::new()) {
        entry.message = bounded_message(&format!(
            "{}; verified original sibling stage was retained at {} ({error})",
            entry.message,
            source.current_path().display()
        ));
        let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
    }
    ApplyOneResult::Applied
}

#[cfg(windows)]
fn compensate_original(
    source: &mut GuardedReplaceFile,
    original_state: file_operations::GuardedReplaceState,
    original_leaf: &std::ffi::OsStr,
) -> std::io::Result<PathBuf> {
    source.rename_to_leaf(original_leaf, original_state, &ScanCancellation::new())
}

#[cfg(windows)]
fn compensate_before_publish(
    source: &mut GuardedReplaceFile,
    output: &GuardedReplaceStage,
    original_state: file_operations::GuardedReplaceState,
    original_leaf: &std::ffi::OsStr,
    entry_index: usize,
    entry: &mut ReplaceRecoveryEntry,
    journal: &mut ReplaceRecoveryJournal,
    journal_path: &Path,
    receipt: &mut ReplaceRecoveryReceipt,
    reason: String,
) -> ApplyOneResult {
    let compensation = compensate_original(source, original_state, original_leaf);
    let stage_cleanup = if compensation.is_ok() {
        output.delete_if_matches(&ScanCancellation::new())
    } else {
        Err(std::io::Error::other(
            "original could not be returned to its recorded path",
        ))
    };
    if compensation.is_ok() && stage_cleanup.is_ok() {
        entry.phase = ReplaceEntryPhase::Failed;
        entry.outcome = ReplaceEntryOutcome::Failed;
        entry.message = bounded_message(&format!(
            "{reason}; original safely returned to its original path"
        ));
        if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, entry) {
            return ApplyOneResult::Refused {
                message: format!("{}; result save failed: {error}", entry.message),
                recovery_required: true,
            };
        }
        ApplyOneResult::Refused {
            message: entry.message.clone(),
            recovery_required: false,
        }
    } else {
        entry.phase = ReplaceEntryPhase::RecoveryRequired;
        entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
        entry.message = bounded_message(&format!(
            "{reason}; original compensation: {compensation:?}; output stage cleanup: {stage_cleanup:?}"
        ));
        let _ = persist_entry(journal, journal_path, receipt, entry_index, entry);
        ApplyOneResult::Refused {
            message: entry.message.clone(),
            recovery_required: true,
        }
    }
}

#[cfg(windows)]
fn cancel_between_moves(
    source: &mut GuardedReplaceFile,
    output: &GuardedReplaceStage,
    original_state: file_operations::GuardedReplaceState,
    original_leaf: &std::ffi::OsStr,
    entry_index: usize,
    entry: &mut ReplaceRecoveryEntry,
    journal: &mut ReplaceRecoveryJournal,
    journal_path: &Path,
    receipt: &mut ReplaceRecoveryReceipt,
) -> ApplyOneResult {
    let compensation = compensate_original(source, original_state, original_leaf);
    let stage_cleanup = if compensation.is_ok() {
        output.delete_if_matches(&ScanCancellation::new())
    } else {
        Err(std::io::Error::other(
            "original could not be returned to its recorded path",
        ))
    };
    if compensation.is_ok() && stage_cleanup.is_ok() {
        entry.phase = ReplaceEntryPhase::Cancelled;
        entry.outcome = ReplaceEntryOutcome::Cancelled;
        entry.message =
            "cancelled between safe moves; original restored to its path and output stage removed"
                .into();
        let saved = persist_entry(journal, journal_path, receipt, entry_index, entry);
        let (message, save_failed) = match saved {
            Ok(()) => (entry.message.clone(), false),
            Err(error) => (
                format!("{}; result save failed: {error}", entry.message),
                true,
            ),
        };
        ApplyOneResult::Cancelled {
            message,
            recovery_required: save_failed,
        }
    } else {
        entry.phase = ReplaceEntryPhase::RecoveryRequired;
        entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
        entry.message = bounded_message(&format!(
            "cancelled between moves; original compensation: {compensation:?}; output stage cleanup: {stage_cleanup:?}"
        ));
        let _ = persist_entry(journal, journal_path, receipt, entry_index, entry);
        ApplyOneResult::Cancelled {
            message: entry.message.clone(),
            recovery_required: true,
        }
    }
}

fn persist_entry(
    journal: &mut ReplaceRecoveryJournal,
    journal_path: &Path,
    receipt: &mut ReplaceRecoveryReceipt,
    entry_index: usize,
    entry: &ReplaceRecoveryEntry,
) -> Result<(), String> {
    let Some(target) = receipt.entries.get_mut(entry_index) else {
        return Err("replacement receipt entry index is invalid".into());
    };
    *target = entry.clone();
    persist_receipt(journal, journal_path, receipt)
}

fn persist_receipt(
    journal: &mut ReplaceRecoveryJournal,
    journal_path: &Path,
    receipt: &ReplaceRecoveryReceipt,
) -> Result<(), String> {
    update_receipt(journal, journal_path, receipt)
}

fn update_receipt(
    journal: &mut ReplaceRecoveryJournal,
    journal_path: &Path,
    receipt: &ReplaceRecoveryReceipt,
) -> Result<(), String> {
    let Some(existing) = journal
        .receipts
        .iter_mut()
        .find(|existing| existing.id == receipt.id)
    else {
        return Err("replacement receipt is missing from its journal".into());
    };
    *existing = receipt.clone();
    save_replace_journal(journal_path, journal)
}

fn mark_remaining(
    receipt: &mut ReplaceRecoveryReceipt,
    start: usize,
    outcome: ReplaceEntryOutcome,
    phase: ReplaceEntryPhase,
    message: &str,
) {
    for entry in receipt.entries.iter_mut().skip(start) {
        if entry.outcome == ReplaceEntryOutcome::Pending {
            entry.outcome = outcome;
            entry.phase = phase;
            entry.message = bounded_message(message);
        }
    }
}

fn failed_receipt(mut receipt: ReplaceRecoveryReceipt, message: String) -> ReplaceOperationResult {
    receipt.status = ReplaceTransactionStatus::Failed;
    receipt.message = bounded_message(&message);
    ReplaceOperationResult {
        receipt,
        message: bounded_message(&message),
    }
}

fn refused_receipt(
    mut receipt: ReplaceRecoveryReceipt,
    message: impl Into<String>,
) -> ReplaceOperationResult {
    let message = bounded_message(&message.into());
    receipt.status = ReplaceTransactionStatus::Refused;
    receipt.message = message.clone();
    ReplaceOperationResult { receipt, message }
}

fn refused_result(transaction_id: &str, message: String) -> ReplaceOperationResult {
    let message = bounded_message(&message);
    let receipt = ReplaceRecoveryReceipt {
        id: transaction_id.to_owned(),
        search_revision: 0,
        search_generation: 0,
        replacement_revision: 0,
        status: ReplaceTransactionStatus::Refused,
        entries: Vec::new(),
        message: message.clone(),
    };
    ReplaceOperationResult { receipt, message }
}

fn bounded_message(message: &str) -> String {
    message.chars().take(MAX_MESSAGE_CHARS).collect()
}

fn role_leaf(transaction_id: &str, index: usize, role: &str) -> String {
    format!("ml-replace-{transaction_id}-{index:03}-{role}.stage")
}

fn is_confined_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn load_replace_journal(path: &Path) -> Result<ReplaceRecoveryJournal, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ReplaceRecoveryJournal::default());
        }
        Err(error) => {
            return Err(format!(
                "replacement recovery journal could not be inspected: {error}"
            ));
        }
    };
    if !metadata.is_file() || metadata_is_reparse_point(&metadata) {
        return Err("replacement recovery journal is not a regular application-owned file".into());
    }
    if metadata.len() > MAX_REPLACE_JOURNAL_BYTES as u64 {
        return Err(
            "replacement recovery journal exceeds its strict size limit; its bytes were preserved"
                .into(),
        );
    }
    let file = std::fs::File::open(path)
        .map_err(|error| format!("replacement recovery journal could not be opened: {error}"))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_REPLACE_JOURNAL_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("replacement recovery journal could not be read: {error}"))?;
    if bytes.len() > MAX_REPLACE_JOURNAL_BYTES {
        return Err("replacement recovery journal grew beyond its strict size limit; its bytes were preserved".into());
    }
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(
            "replacement recovery journal is empty or whitespace-only; its bytes were preserved"
                .into(),
        );
    }
    let journal: ReplaceRecoveryJournal = serde_json::from_slice(&bytes)
        .map_err(|error| format!("replacement recovery journal is malformed or uses an unknown schema; its bytes were preserved: {error}"))?;
    validate_replace_journal(&journal)?;
    Ok(journal)
}

fn save_replace_journal(path: &Path, journal: &ReplaceRecoveryJournal) -> Result<(), String> {
    validate_replace_journal(journal)?;
    let bytes = serde_json::to_vec_pretty(journal)
        .map_err(|error| format!("replacement recovery journal could not be encoded: {error}"))?;
    if bytes.len() > MAX_REPLACE_JOURNAL_BYTES {
        return Err("replacement recovery journal would exceed its strict size limit".into());
    }
    save_json_atomic(path, journal).map_err(|error| {
        format!("replacement recovery journal could not be durably saved: {error}")
    })?;
    let verified = load_replace_journal(path)?;
    if &verified != journal {
        return Err("replacement recovery journal did not verify after its durable save".into());
    }
    Ok(())
}

fn validate_replace_journal(journal: &ReplaceRecoveryJournal) -> Result<(), String> {
    if journal.schema_version != REPLACE_JOURNAL_SCHEMA_VERSION {
        return Err(
            "replacement recovery journal uses an unsupported schema; its bytes were preserved"
                .into(),
        );
    }
    if journal.receipts.len() > MAX_RETAINED_REPLACE_RECEIPTS {
        return Err("replacement recovery journal exceeds its receipt-count limit".into());
    }
    let mut receipt_ids = HashSet::new();
    for receipt in &journal.receipts {
        if !valid_transaction_id(&receipt.id) || !receipt_ids.insert(receipt.id.as_str()) {
            return Err("replacement recovery journal contains an invalid or duplicate transaction identity".into());
        }
        if receipt.entries.is_empty() || receipt.entries.len() > MAX_REPLACE_FILES_PER_BATCH {
            return Err("replacement recovery receipt has an invalid file count".into());
        }
        if receipt.message.chars().count() > MAX_MESSAGE_CHARS {
            return Err("replacement recovery receipt message exceeds its size limit".into());
        }
        let mut total_bytes = 0_u64;
        for (index, entry) in receipt.entries.iter().enumerate() {
            if !entry.root_path.is_absolute()
                || entry.root_path.to_str().is_none()
                || !is_confined_relative_path(&entry.relative_path)
                || entry.relative_path.to_str().is_none()
                || !entry.path().is_absolute()
            {
                return Err(
                    "replacement recovery receipt contains an unconfined or unsupported path"
                        .into(),
                );
            }
            if entry.original_length > MAX_RENAME_HASH_BYTES_PER_FILE
                || entry.output_length > MAX_RENAME_HASH_BYTES_PER_FILE
            {
                return Err(
                    "replacement recovery receipt exceeds its per-file payload bound".into(),
                );
            }
            total_bytes = total_bytes
                .checked_add(entry.original_length)
                .and_then(|sum| sum.checked_add(entry.output_length))
                .ok_or_else(|| "replacement recovery byte count overflowed".to_owned())?;
            if total_bytes > MAX_RENAME_HASH_BYTES_PER_BATCH {
                return Err("replacement recovery receipt exceeds its batch payload bound".into());
            }
            if entry.backup_leaf != role_leaf(&receipt.id, index, "backup")
                || entry.output_stage_leaf != role_leaf(&receipt.id, index, "output")
                || entry.original_stage_leaf != role_leaf(&receipt.id, index, "source")
                || entry.restore_stage_leaf != role_leaf(&receipt.id, index, "restore")
                || entry.restore_source_stage_leaf != role_leaf(&receipt.id, index, "post")
                || [
                    &entry.backup_leaf,
                    &entry.output_stage_leaf,
                    &entry.original_stage_leaf,
                    &entry.restore_stage_leaf,
                    &entry.restore_source_stage_leaf,
                ]
                .iter()
                .any(|leaf| {
                    Path::new(leaf).components().count() != 1
                        || Path::new(leaf).file_name().is_none()
                })
            {
                return Err("replacement recovery receipt contains an untrusted stage role".into());
            }
            for message in [&entry.message] {
                if message.chars().count() > MAX_MESSAGE_CHARS {
                    return Err("replacement recovery entry message exceeds its size limit".into());
                }
            }
        }
    }
    Ok(())
}

fn valid_transaction_id(transaction_id: &str) -> bool {
    transaction_id.len() == 32
        && transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn ensure_recovery_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path.to_str().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(
            "replacement recovery directory must be an absolute confined Unicode path".into(),
        );
    }
    let Some(parent) = path.parent() else {
        return Err("replacement recovery directory has no parent".into());
    };
    verify_directory_chain(parent)?;
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(format!(
                "replacement recovery directory could not be created: {error}"
            ));
        }
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        format!("replacement recovery directory could not be inspected: {error}")
    })?;
    if !metadata.is_dir() || metadata_is_reparse_point(&metadata) {
        return Err(
            "replacement recovery location is not a plain application-owned directory".into(),
        );
    }
    Ok(path.to_path_buf())
}

fn verify_directory_chain(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                current.push(component.as_os_str());
            }
            Component::CurDir => continue,
            Component::ParentDir => {
                return Err("replacement recovery path contains parent traversal".into());
            }
        }
        if current.as_os_str().is_empty() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&current).map_err(|error| {
            format!(
                "replacement recovery parent {} is unavailable: {error}",
                current.display()
            )
        })?;
        if !metadata.is_dir() || metadata_is_reparse_point(&metadata) {
            return Err(format!(
                "replacement recovery parent {} is not a plain directory",
                current.display()
            ));
        }
    }
    Ok(())
}

fn metadata_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(windows)]
#[derive(Debug)]
enum RestoreOneResult {
    Restored,
    Cancelled {
        message: String,
        recovery_required: bool,
    },
    Refused {
        message: String,
        recovery_required: bool,
        committed: bool,
    },
}

#[cfg(windows)]
impl RestoreOneResult {
    fn refused(message: String) -> Self {
        Self::Refused {
            message,
            recovery_required: false,
            committed: false,
        }
    }

    fn recovery(message: impl Into<String>, committed: bool) -> Self {
        Self::Refused {
            message: message.into(),
            recovery_required: true,
            committed,
        }
    }
}

#[cfg(windows)]
fn restore_one_entry(
    entry_index: usize,
    receipt: &mut ReplaceRecoveryReceipt,
    journal: &mut ReplaceRecoveryJournal,
    receipt_index: usize,
    journal_path: &Path,
    recovery_directory: &Path,
    cancellation: &ScanCancellation,
) -> RestoreOneResult {
    let mut entry = receipt.entries[entry_index].clone();
    let Some(post_identity) = entry.post_identity else {
        return RestoreOneResult::refused(
            "the receipt has no verified post-write file identity; backup retained".into(),
        );
    };
    let Some(backup_identity) = entry.backup_identity else {
        return RestoreOneResult::refused(
            "the receipt has no verified original backup identity; no restore was attempted".into(),
        );
    };
    if cancellation.is_cancelled() {
        return RestoreOneResult::Cancelled {
            message: "Restore was cancelled before opening the current file".into(),
            recovery_required: false,
        };
    }
    let backup = match file_operations::open_verified_replace_backup(
        &recovery_directory.join(&entry.backup_leaf),
        backup_identity,
        entry.original_length,
        entry.original_sha256,
        cancellation,
    ) {
        Ok(backup) => backup,
        Err(error) => {
            return RestoreOneResult::refused(format!(
                "verified original backup is unavailable or changed; bytes were retained ({error})"
            ));
        }
    };
    if backup.bytes().as_ref().len() as u64 != entry.original_length
        || <[u8; 32]>::from(Sha256::digest(backup.bytes().as_ref())) != entry.original_sha256
    {
        return RestoreOneResult::refused(
            "verified original backup no longer matches the recorded original payload".into(),
        );
    }

    let expected_post_modified = entry.post_modified;
    let read = match read_current_replace_target(
        &entry,
        post_identity,
        entry.output_length,
        expected_post_modified,
        entry.output_sha256,
        cancellation,
    ) {
        Ok(read) => read,
        Err(error) => {
            return RestoreOneResult::refused(format!(
                "the selected post-write file is missing, edited, replaced, or no longer root-confined; backup retained ({error})"
            ));
        }
    };
    let evidence = std::sync::Arc::clone(read.evidence());
    let mut source = match file_operations::prepare_guarded_replace_file(
        &evidence,
        &entry.root_path,
        entry.root_identity,
        &entry.path(),
        post_identity,
        read.parent_identity(),
        entry.output_length,
        read.modified(),
        entry.output_sha256,
        read.bytes(),
        cancellation,
    ) {
        Ok(source) => source,
        Err(error) => {
            return RestoreOneResult::refused(format!(
                "the post-write file became stale before Restore; original backup retained ({})",
                error.message()
            ));
        }
    };
    if cancellation.is_cancelled() {
        return RestoreOneResult::Cancelled {
            message: "Restore was cancelled before creating its verified output stage".into(),
            recovery_required: false,
        };
    }

    entry.phase = ReplaceEntryPhase::RestorePending;
    entry.message = "verified backup and unchanged post-write file are ready".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        return RestoreOneResult::refused(format!(
            "Restore was not staged because its intent could not be saved: {error}"
        ));
    }

    let destination_leaf = match entry.path().file_name() {
        Some(leaf) => leaf.to_os_string(),
        None => {
            return RestoreOneResult::refused("the recorded source path has no file name".into());
        }
    };
    let mut restore_stage = match source.create_output_stage(
        std::ffi::OsStr::new(&entry.restore_stage_leaf),
        backup.bytes(),
        entry.original_sha256,
        Some(entry.original_modified),
        cancellation,
    ) {
        Ok(stage) => stage,
        Err(error) => {
            let message = format!(
                "original bytes could not be staged without replacing an occupant: {error}"
            );
            return if error.to_string().contains("stage remains") {
                RestoreOneResult::recovery(message, false)
            } else {
                RestoreOneResult::refused(message)
            };
        }
    };
    entry.restore_stage_identity = Some(restore_stage.state().identity);
    entry.phase = ReplaceEntryPhase::RestoreStageReady;
    entry.message = "original backup bytes were staged, flushed, and verified".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        let cleanup = restore_stage.delete_if_matches(&ScanCancellation::new());
        let message = format!(
            "Restore stage could not be recorded before moving the current file: {error}; owned stage cleanup: {cleanup:?}"
        );
        if cleanup.is_err() {
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.message = bounded_message(&message);
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return RestoreOneResult::recovery(message, false);
        }
        return RestoreOneResult::refused(message);
    }
    if cancellation.is_cancelled() {
        let cleanup = restore_stage.delete_if_matches(&ScanCancellation::new());
        entry.phase = if cleanup.is_ok() {
            ReplaceEntryPhase::Cancelled
        } else {
            ReplaceEntryPhase::RecoveryRequired
        };
        entry.outcome = if cleanup.is_ok() {
            ReplaceEntryOutcome::Applied
        } else {
            ReplaceEntryOutcome::RecoveryRequired
        };
        entry.message = if cleanup.is_ok() {
            "Restore cancelled before moving the current file; its verified stage was removed"
                .into()
        } else {
            format!(
                "Restore cancelled; verified original stage remains at {} ({cleanup:?})",
                restore_stage.path().display()
            )
        };
        let saved = persist_entry(journal, journal_path, receipt, entry_index, &entry);
        let (message, save_failed) = match saved {
            Ok(()) => (entry.message.clone(), false),
            Err(error) => (
                format!("{}; receipt save failed: {error}", entry.message),
                true,
            ),
        };
        return RestoreOneResult::Cancelled {
            message,
            recovery_required: cleanup.is_err() || save_failed,
        };
    }

    let post_state = source.original_state();
    entry.phase = ReplaceEntryPhase::RestoreSourceMovePending;
    entry.message = "moving the verified post-write file to its owned recovery stage".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        let cleanup = restore_stage.delete_if_matches(&ScanCancellation::new());
        let message = format!(
            "current file was not moved because its intent could not be saved: {error}; original stage cleanup: {cleanup:?}"
        );
        if cleanup.is_err() {
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.message = bounded_message(&message);
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return RestoreOneResult::recovery(message, false);
        }
        return RestoreOneResult::refused(message);
    }
    if let Err(error) = source.rename_to_leaf(
        std::ffi::OsStr::new(&entry.restore_source_stage_leaf),
        post_state,
        cancellation,
    ) {
        if batch_rename::compare_paths_windows_ordinal(source.current_path(), &entry.path())
            != std::cmp::Ordering::Equal
        {
            entry.restore_source_stage_identity = Some(post_identity);
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.message = bounded_message(&format!(
                "post-write file moved but its recovery location could not be verified: {error}"
            ));
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return RestoreOneResult::recovery(entry.message, false);
        }
        let cleanup = restore_stage.delete_if_matches(&ScanCancellation::new());
        let message =
            format!("post-write file was not moved: {error}; original stage cleanup: {cleanup:?}");
        if cleanup.is_err() {
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.message = bounded_message(&message);
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return RestoreOneResult::recovery(message, false);
        }
        return RestoreOneResult::refused(message);
    }
    entry.restore_source_stage_identity = Some(post_identity);
    entry.phase = ReplaceEntryPhase::RestoreSourceMoved;
    entry.message = "verified post-write file is retained in its owned sibling stage".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        let compensation =
            source.rename_to_leaf(&destination_leaf, post_state, &ScanCancellation::new());
        let cleanup = if compensation.is_ok() {
            restore_stage.delete_if_matches(&ScanCancellation::new())
        } else {
            Err(std::io::Error::other(
                "post-write file could not be returned to its selected path",
            ))
        };
        let message = format!(
            "post-write file relocation could not be recorded: {error}; guarded compensation: {compensation:?}; original stage cleanup: {cleanup:?}"
        );
        return if compensation.is_ok() && cleanup.is_ok() {
            RestoreOneResult::refused(message)
        } else {
            RestoreOneResult::recovery(message, false)
        };
    }

    if cancellation.is_cancelled() {
        let compensation =
            source.rename_to_leaf(&destination_leaf, post_state, &ScanCancellation::new());
        let cleanup = if compensation.is_ok() {
            restore_stage.delete_if_matches(&ScanCancellation::new())
        } else {
            Err(std::io::Error::other(
                "post-write file could not be returned to its selected path",
            ))
        };
        let recovery_required = compensation.is_err() || cleanup.is_err();
        entry.phase = if recovery_required {
            ReplaceEntryPhase::RecoveryRequired
        } else {
            ReplaceEntryPhase::Cancelled
        };
        entry.outcome = if recovery_required {
            ReplaceEntryOutcome::RecoveryRequired
        } else {
            ReplaceEntryOutcome::Applied
        };
        entry.message = bounded_message(&format!(
            "Restore cancelled between safe moves; post-write compensation: {compensation:?}; original stage cleanup: {cleanup:?}"
        ));
        let saved = persist_entry(journal, journal_path, receipt, entry_index, &entry);
        let (message, save_failed) = match saved {
            Ok(()) => (entry.message.clone(), false),
            Err(error) => (
                format!("{}; receipt save failed: {error}", entry.message),
                true,
            ),
        };
        return RestoreOneResult::Cancelled {
            message,
            recovery_required: recovery_required || save_failed,
        };
    }

    entry.phase = ReplaceEntryPhase::RestorePublishPending;
    entry.message = "publishing the verified original stage without replacing any occupant".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        let compensation =
            source.rename_to_leaf(&destination_leaf, post_state, &ScanCancellation::new());
        let cleanup = if compensation.is_ok() {
            restore_stage.delete_if_matches(&ScanCancellation::new())
        } else {
            Err(std::io::Error::other(
                "post-write file could not be returned to its selected path",
            ))
        };
        let message = format!(
            "Restore publication intent could not be saved: {error}; guarded compensation: {compensation:?}; original stage cleanup: {cleanup:?}"
        );
        return if compensation.is_ok() && cleanup.is_ok() {
            RestoreOneResult::refused(message)
        } else {
            RestoreOneResult::recovery(message, false)
        };
    }
    if let Err(error) = restore_stage.rename_to_leaf(&destination_leaf, cancellation) {
        if batch_rename::compare_paths_windows_ordinal(restore_stage.path(), &entry.path())
            == std::cmp::Ordering::Equal
        {
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.restored_identity = Some(restore_stage.state().identity);
            entry.restored_modified = Some(restore_stage.state().modified);
            entry.message = bounded_message(&format!(
                "original stage moved but final verification failed; both payloads are retained: {error}"
            ));
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return RestoreOneResult::recovery(entry.message, true);
        }
        let compensation =
            source.rename_to_leaf(&destination_leaf, post_state, &ScanCancellation::new());
        let cleanup = if compensation.is_ok() {
            restore_stage.delete_if_matches(&ScanCancellation::new())
        } else {
            Err(std::io::Error::other(
                "post-write file could not be returned to its selected path",
            ))
        };
        let recovery_required = compensation.is_err() || cleanup.is_err();
        entry.phase = if recovery_required {
            ReplaceEntryPhase::RecoveryRequired
        } else {
            ReplaceEntryPhase::Failed
        };
        entry.outcome = if recovery_required {
            ReplaceEntryOutcome::RecoveryRequired
        } else {
            ReplaceEntryOutcome::Applied
        };
        entry.message = bounded_message(&format!(
            "original stage was not published: {error}; post-write compensation: {compensation:?}; original stage cleanup: {cleanup:?}"
        ));
        let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
        return if recovery_required {
            RestoreOneResult::recovery(entry.message, false)
        } else {
            RestoreOneResult::refused(entry.message)
        };
    }

    let restored = match restore_stage.verify_at(&entry.path(), &ScanCancellation::new()) {
        Ok(state)
            if state.identity == entry.restore_stage_identity.unwrap_or_default()
                && state.length == entry.original_length
                && state.content_sha256 == entry.original_sha256
                && state.modified == entry.original_modified =>
        {
            state
        }
        Ok(_) => {
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.restored_identity = Some(restore_stage.state().identity);
            entry.message =
                "restored source path did not match the verified original payload and metadata"
                    .into();
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return RestoreOneResult::recovery(entry.message, true);
        }
        Err(error) => {
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.restored_identity = Some(restore_stage.state().identity);
            entry.message = bounded_message(&format!(
                "original payload was published but could not be verified; post-write payload remains retained: {error}"
            ));
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            return RestoreOneResult::recovery(entry.message, true);
        }
    };
    entry.restored_identity = Some(restored.identity);
    entry.restored_modified = Some(restored.modified);
    entry.phase = ReplaceEntryPhase::Restored;
    entry.outcome = ReplaceEntryOutcome::Restored;
    entry.message =
        "original backup bytes were restored and verified; durable backup retained".into();
    if let Err(error) = persist_entry(journal, journal_path, receipt, entry_index, &entry) {
        return RestoreOneResult::recovery(
            format!("restored source is verified, but its receipt could not be saved: {error}"),
            true,
        );
    }

    if let Err(error) = source.delete_if_matches(post_state, &ScanCancellation::new()) {
        entry.phase = ReplaceEntryPhase::RecoveryRequired;
        entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
        entry.message = bounded_message(&format!(
            "{}; post-write recovery stage retained at {} ({error})",
            entry.message,
            source.current_path().display()
        ));
        let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
        return RestoreOneResult::recovery(entry.message, true);
    }
    RestoreOneResult::Restored
}

#[cfg(windows)]
fn read_current_replace_target(
    entry: &ReplaceRecoveryEntry,
    expected_identity: NativeFileIdentity,
    expected_length: u64,
    expected_modified: Option<SystemTime>,
    expected_sha256: [u8; 32],
    cancellation: &ScanCancellation,
) -> Result<VerifiedReadSnapshot, String> {
    read_recorded_replace_path(
        entry,
        &entry.path(),
        expected_identity,
        expected_length,
        expected_modified,
        expected_sha256,
        cancellation,
    )
}

#[cfg(windows)]
fn read_recorded_replace_path(
    entry: &ReplaceRecoveryEntry,
    selected_path: &Path,
    expected_identity: NativeFileIdentity,
    expected_length: u64,
    expected_modified: Option<SystemTime>,
    expected_sha256: [u8; 32],
    cancellation: &ScanCancellation,
) -> Result<VerifiedReadSnapshot, String> {
    if selected_path.parent().is_none_or(|parent| {
        batch_rename::compare_paths_windows_ordinal(
            parent,
            entry.path().parent().unwrap_or_else(|| Path::new("")),
        ) != std::cmp::Ordering::Equal
    }) {
        return Err("recorded recovery role is not an adjacent sibling of its source".into());
    }
    let root = ExplicitDirectoryRoot::new(entry.root_path.clone())
        .map_err(|error| format!("recorded root is invalid: {error:?}"))?;
    let generation = RenameScanGeneration(RESTORE_SCAN_GENERATION.fetch_add(1, Ordering::Relaxed));
    let scan = file_operations::scan_rename_root(
        RenameScanRequest::new(root, true, generation),
        cancellation,
    );
    if scan.status() != RenameScanStatus::Complete
        || scan.root_identity() != Some(entry.root_identity)
    {
        return Err("recorded root is missing, changed, or no longer completely enumerable".into());
    }
    let Some(row) = scan.rows().iter().find(|row| {
        batch_rename::compare_paths_windows_ordinal(row.path(), selected_path)
            == std::cmp::Ordering::Equal
    }) else {
        return Err("recorded recovery role is missing from the current root scan".into());
    };
    if row.kind() != SnapshotEntryKind::File
        || row.is_reparse_point()
        || row.identity() != expected_identity
        || row.parent_identity() != entry.parent_identity
        || row.length() != Some(expected_length)
        || expected_modified.is_some_and(|modified| row.modified() != Some(modified))
    {
        return Err(
            "recorded recovery role identity, parent, length, or modification time changed".into(),
        );
    }
    let read = file_operations::read_verified_scan_file(
        &scan,
        row,
        MAX_RENAME_HASH_BYTES_PER_FILE,
        cancellation,
    )
    .map_err(|error| error.message().to_owned())?;
    if read.content_sha256() != expected_sha256
        || read.length() != expected_length
        || expected_modified.is_some_and(|modified| read.modified() != modified)
        || read.identity() != expected_identity
        || read.parent_identity() != entry.parent_identity
    {
        return Err(
            "recorded recovery role content hash or verified source metadata changed".into(),
        );
    }
    Ok(read)
}

#[cfg(windows)]
fn recover_interrupted_apply(
    entry_index: usize,
    receipt: &mut ReplaceRecoveryReceipt,
    journal: &mut ReplaceRecoveryJournal,
    receipt_index: usize,
    journal_path: &Path,
    recovery_directory: &Path,
    cancellation: &ScanCancellation,
) -> Result<(), String> {
    let mut entry = receipt.entries[entry_index].clone();
    let original_leaf = entry
        .path()
        .file_name()
        .ok_or_else(|| "recorded source path has no final component".to_owned())?
        .to_os_string();

    if let Some(backup_identity) = entry.backup_identity {
        let _backup = file_operations::open_verified_replace_backup(
            &recovery_directory.join(&entry.backup_leaf),
            backup_identity,
            entry.original_length,
            entry.original_sha256,
            cancellation,
        )
        .map_err(|error| format!("recorded original backup is not verified: {error}"))?;
    }

    let output_identity = entry.post_identity.or(entry.output_stage_identity);
    if let Some(output_identity) = output_identity {
        if let Ok(published) = read_recorded_replace_path(
            &entry,
            &entry.path(),
            output_identity,
            entry.output_length,
            entry.post_modified,
            entry.output_sha256,
            cancellation,
        ) {
            if entry.backup_identity.is_none() {
                return Err("published output is recorded but its durable original backup identity is missing".into());
            }
            entry.post_identity = Some(published.identity());
            entry.post_modified = Some(published.modified());
            entry.phase = ReplaceEntryPhase::Applied;
            entry.outcome = ReplaceEntryOutcome::Applied;
            entry.message =
                "reconciled the exact recorded output already published at its selected path"
                    .into();
            persist_entry(journal, journal_path, receipt, entry_index, &entry)?;
            if let Err(error) = cleanup_recorded_original_stage(&entry, cancellation) {
                entry.message = bounded_message(&format!(
                    "{}; verified original sibling stage was retained ({error})",
                    entry.message
                ));
                let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
            }
            return Ok(());
        }
    }

    if let Ok(original_at_target) = read_recorded_replace_path(
        &entry,
        &entry.path(),
        entry.original_identity,
        entry.original_length,
        Some(entry.original_modified),
        entry.original_sha256,
        cancellation,
    ) {
        entry.phase = ReplaceEntryPhase::Failed;
        entry.outcome = ReplaceEntryOutcome::Failed;
        entry.message = "reconciled that the original remains unchanged at its selected path; no source mutation occurred".into();
        let _ = original_at_target;
        persist_entry(journal, journal_path, receipt, entry_index, &entry)?;
        return Ok(());
    }

    let backup_identity = entry.backup_identity.ok_or_else(|| {
        "interrupted source relocation cannot be completed without its verified durable backup identity".to_owned()
    })?;
    let _backup = file_operations::open_verified_replace_backup(
        &recovery_directory.join(&entry.backup_leaf),
        backup_identity,
        entry.original_length,
        entry.original_sha256,
        cancellation,
    )
    .map_err(|error| format!("recorded original backup is not verified: {error}"))?;

    let source_identity = entry
        .original_stage_identity
        .unwrap_or(entry.original_identity);
    let source_stage_path = entry
        .path()
        .parent()
        .ok_or_else(|| "recorded source path has no parent directory".to_owned())?
        .join(&entry.original_stage_leaf);
    let original_stage = read_recorded_replace_path(
        &entry,
        &source_stage_path,
        source_identity,
        entry.original_length,
        None,
        entry.original_sha256,
        cancellation,
    )
    .map_err(|error| format!("recorded original sibling stage is not verified: {error}"))?;
    let output_identity = entry.output_stage_identity.ok_or_else(|| {
        "interrupted publication has no recorded output-stage identity".to_owned()
    })?;
    let output_stage_path = entry
        .path()
        .parent()
        .ok_or_else(|| "recorded source path has no parent directory".to_owned())?
        .join(&entry.output_stage_leaf);
    let output_stage = read_recorded_replace_path(
        &entry,
        &output_stage_path,
        output_identity,
        entry.output_length,
        None,
        entry.output_sha256,
        cancellation,
    )
    .map_err(|error| format!("recorded exact output stage is not verified: {error}"))?;

    let original_guard = file_operations::prepare_guarded_replace_file(
        original_stage.evidence(),
        &entry.root_path,
        entry.root_identity,
        &source_stage_path,
        source_identity,
        original_stage.parent_identity(),
        entry.original_length,
        original_stage.modified(),
        entry.original_sha256,
        original_stage.bytes(),
        cancellation,
    )
    .map_err(|error| {
        format!(
            "recorded original stage changed during recovery admission: {}",
            error.message()
        )
    })?;
    let mut output_guard = file_operations::prepare_guarded_replace_file(
        output_stage.evidence(),
        &entry.root_path,
        entry.root_identity,
        &output_stage_path,
        output_identity,
        output_stage.parent_identity(),
        entry.output_length,
        output_stage.modified(),
        entry.output_sha256,
        output_stage.bytes(),
        cancellation,
    )
    .map_err(|error| {
        format!(
            "recorded output stage changed during recovery admission: {}",
            error.message()
        )
    })?;

    entry.original_stage_identity = Some(source_identity);
    entry.phase = ReplaceEntryPhase::PublishPending;
    entry.message =
        "verified output and original stages are ready for guarded recovery publication".into();
    persist_entry(journal, journal_path, receipt, entry_index, &entry)?;
    if cancellation.is_cancelled() {
        return Err("recovery was cancelled before the guarded output publication".into());
    }
    let output_state = output_guard.original_state();
    if let Err(error) = output_guard.rename_to_leaf(&original_leaf, output_state, cancellation) {
        if batch_rename::compare_paths_windows_ordinal(output_guard.current_path(), &entry.path())
            == std::cmp::Ordering::Equal
        {
            entry.post_identity = Some(output_state.identity);
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.message = bounded_message(&format!(
                "recorded output stage moved to the selected path but verification failed: {error}"
            ));
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
        }
        return Err(format!(
            "guarded recovery publication did not complete: {error}"
        ));
    }
    let published = output_guard
        .verify_at(
            &entry.path(),
            entry.output_length,
            None,
            entry.output_sha256,
            None,
            &ScanCancellation::new(),
        )
        .map_err(|error| {
            format!(
                "recovered output moved but could not be verified at its selected path: {error}"
            )
        })?;
    entry.post_identity = Some(published.identity);
    entry.post_modified = Some(published.modified);
    entry.phase = ReplaceEntryPhase::Applied;
    entry.outcome = ReplaceEntryOutcome::Applied;
    entry.message =
        "exact recorded output was published by guarded recovery; durable original backup retained"
            .into();
    persist_entry(journal, journal_path, receipt, entry_index, &entry)?;

    if let Err(error) =
        original_guard.delete_if_matches(original_guard.original_state(), &ScanCancellation::new())
    {
        entry.message = bounded_message(&format!(
            "{}; verified duplicate original stage retained at {} ({error})",
            entry.message,
            original_guard.current_path().display()
        ));
        let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
    }
    Ok(())
}

#[cfg(windows)]
fn cleanup_recorded_original_stage(
    entry: &ReplaceRecoveryEntry,
    cancellation: &ScanCancellation,
) -> Result<(), String> {
    let identity = entry
        .original_stage_identity
        .unwrap_or(entry.original_identity);
    let stage_path = entry
        .path()
        .parent()
        .ok_or_else(|| "recorded source path has no parent directory".to_owned())?
        .join(&entry.original_stage_leaf);
    let stage = match read_recorded_replace_path(
        entry,
        &stage_path,
        identity,
        entry.original_length,
        None,
        entry.original_sha256,
        cancellation,
    ) {
        Ok(stage) => stage,
        Err(error) if error.contains("recorded recovery role is missing") => return Ok(()),
        Err(error) => return Err(error),
    };
    let guard = file_operations::prepare_guarded_replace_file(
        stage.evidence(),
        &entry.root_path,
        entry.root_identity,
        &stage_path,
        identity,
        stage.parent_identity(),
        entry.original_length,
        stage.modified(),
        entry.original_sha256,
        stage.bytes(),
        cancellation,
    )
    .map_err(|error| {
        format!(
            "recorded original sibling stage changed before cleanup: {}",
            error.message()
        )
    })?;
    guard
        .delete_if_matches(guard.original_state(), &ScanCancellation::new())
        .map_err(|error| format!("recorded original sibling stage could not be removed: {error}"))
}

#[cfg(windows)]
fn recover_interrupted_restore(
    entry_index: usize,
    receipt: &mut ReplaceRecoveryReceipt,
    journal: &mut ReplaceRecoveryJournal,
    receipt_index: usize,
    journal_path: &Path,
    recovery_directory: &Path,
    cancellation: &ScanCancellation,
) -> Result<(), String> {
    let mut entry = receipt.entries[entry_index].clone();
    let backup_identity = entry.backup_identity.ok_or_else(|| {
        "interrupted Restore has no recorded durable original backup identity".to_owned()
    })?;
    let _backup = file_operations::open_verified_replace_backup(
        &recovery_directory.join(&entry.backup_leaf),
        backup_identity,
        entry.original_length,
        entry.original_sha256,
        cancellation,
    )
    .map_err(|error| format!("recorded original backup is not verified: {error}"))?;

    if let Some(restored_identity) = entry.restored_identity.or(entry.restore_stage_identity) {
        if let Ok(restored) = read_recorded_replace_path(
            &entry,
            &entry.path(),
            restored_identity,
            entry.original_length,
            entry.restored_modified,
            entry.original_sha256,
            cancellation,
        ) {
            entry.restored_identity = Some(restored.identity());
            entry.restored_modified = Some(restored.modified());
            entry.phase = ReplaceEntryPhase::Restored;
            entry.outcome = ReplaceEntryOutcome::Restored;
            entry.message =
                "reconciled the exact restored original already at its selected path".into();
            persist_entry(journal, journal_path, receipt, entry_index, &entry)?;
            return Ok(());
        }
    }

    let post_identity = entry
        .post_identity
        .ok_or_else(|| "interrupted Restore has no recorded post-write identity".to_owned())?;
    if read_recorded_replace_path(
        &entry,
        &entry.path(),
        post_identity,
        entry.output_length,
        entry.post_modified,
        entry.output_sha256,
        cancellation,
    )
    .is_ok()
    {
        entry.phase = ReplaceEntryPhase::Applied;
        entry.outcome = ReplaceEntryOutcome::Applied;
        entry.message =
            "reconciled that Restore had not moved the unchanged post-write file".into();
        persist_entry(journal, journal_path, receipt, entry_index, &entry)?;
        if let Some(stage_identity) = entry.restore_stage_identity {
            let stage_path = entry
                .path()
                .parent()
                .ok_or_else(|| "recorded source path has no parent directory".to_owned())?
                .join(&entry.restore_stage_leaf);
            let stage = match read_recorded_replace_path(
                &entry,
                &stage_path,
                stage_identity,
                entry.original_length,
                None,
                entry.original_sha256,
                cancellation,
            ) {
                Ok(stage) => stage,
                Err(error) if error.contains("recorded recovery role is missing") => return Ok(()),
                Err(error) => {
                    return Err(format!(
                        "recorded Restore stage cannot be safely cleaned: {error}"
                    ));
                }
            };
            let stage_guard = file_operations::prepare_guarded_replace_file(
                stage.evidence(),
                &entry.root_path,
                entry.root_identity,
                &stage_path,
                stage_identity,
                stage.parent_identity(),
                entry.original_length,
                stage.modified(),
                entry.original_sha256,
                stage.bytes(),
                cancellation,
            )
            .map_err(|error| {
                format!(
                    "recorded Restore stage changed before cleanup: {}",
                    error.message()
                )
            })?;
            stage_guard
                .delete_if_matches(stage_guard.original_state(), &ScanCancellation::new())
                .map_err(|error| format!("verified Restore stage could not be removed: {error}"))?;
        }
        return Ok(());
    }

    let stage_identity = entry.restore_stage_identity.ok_or_else(|| {
        "interrupted Restore has no recorded original Restore-stage identity".to_owned()
    })?;
    let stage_path = entry
        .path()
        .parent()
        .ok_or_else(|| "recorded source path has no parent directory".to_owned())?
        .join(&entry.restore_stage_leaf);
    let restore_stage = read_recorded_replace_path(
        &entry,
        &stage_path,
        stage_identity,
        entry.original_length,
        None,
        entry.original_sha256,
        cancellation,
    )
    .map_err(|error| format!("recorded original Restore stage is not verified: {error}"))?;

    let post_stage_identity = entry.restore_source_stage_identity.unwrap_or(post_identity);
    let post_stage_path = entry
        .path()
        .parent()
        .ok_or_else(|| "recorded source path has no parent directory".to_owned())?
        .join(&entry.restore_source_stage_leaf);
    let post_stage = read_recorded_replace_path(
        &entry,
        &post_stage_path,
        post_stage_identity,
        entry.output_length,
        None,
        entry.output_sha256,
        cancellation,
    )
    .map_err(|error| format!("recorded post-write recovery stage is not verified: {error}"))?;
    let mut restore_guard = file_operations::prepare_guarded_replace_file(
        restore_stage.evidence(),
        &entry.root_path,
        entry.root_identity,
        &stage_path,
        stage_identity,
        restore_stage.parent_identity(),
        entry.original_length,
        restore_stage.modified(),
        entry.original_sha256,
        restore_stage.bytes(),
        cancellation,
    )
    .map_err(|error| {
        format!(
            "recorded Restore stage changed during recovery admission: {}",
            error.message()
        )
    })?;
    let _post_guard = file_operations::prepare_guarded_replace_file(
        post_stage.evidence(),
        &entry.root_path,
        entry.root_identity,
        &post_stage_path,
        post_stage_identity,
        post_stage.parent_identity(),
        entry.output_length,
        post_stage.modified(),
        entry.output_sha256,
        post_stage.bytes(),
        cancellation,
    )
    .map_err(|error| {
        format!(
            "recorded post-write recovery stage changed during admission: {}",
            error.message()
        )
    })?;

    let destination_leaf = entry
        .path()
        .file_name()
        .ok_or_else(|| "recorded source path has no final component".to_owned())?
        .to_os_string();
    entry.phase = ReplaceEntryPhase::RestorePublishPending;
    entry.message =
        "verified original and post-write payloads are ready for guarded Restore recovery".into();
    persist_entry(journal, journal_path, receipt, entry_index, &entry)?;
    if cancellation.is_cancelled() {
        return Err("recovery was cancelled before guarded Restore publication".into());
    }
    let restore_state = restore_guard.original_state();
    if let Err(error) = restore_guard.rename_to_leaf(&destination_leaf, restore_state, cancellation)
    {
        if batch_rename::compare_paths_windows_ordinal(restore_guard.current_path(), &entry.path())
            == std::cmp::Ordering::Equal
        {
            entry.restored_identity = Some(restore_state.identity);
            entry.phase = ReplaceEntryPhase::RecoveryRequired;
            entry.outcome = ReplaceEntryOutcome::RecoveryRequired;
            entry.message = bounded_message(&format!(
                "recorded Restore stage moved to the selected path but verification failed: {error}"
            ));
            let _ = persist_entry(journal, journal_path, receipt, entry_index, &entry);
        }
        return Err(format!(
            "guarded Restore recovery publication did not complete: {error}"
        ));
    }
    let restored = restore_guard
        .verify_at(
            &entry.path(),
            entry.original_length,
            Some(entry.original_modified),
            entry.original_sha256,
            None,
            &ScanCancellation::new(),
        )
        .map_err(|error| format!("recovered original moved but could not be verified: {error}"))?;
    entry.restored_identity = Some(restored.identity);
    entry.restored_modified = Some(restored.modified);
    entry.phase = ReplaceEntryPhase::Restored;
    entry.outcome = ReplaceEntryOutcome::Restored;
    entry.message = "verified original was restored from its recorded stage; durable backup and post-write stage retained".into();
    persist_entry(journal, journal_path, receipt, entry_index, &entry)
}

#[cfg(windows)]
fn verify_already_restored(
    entry: &ReplaceRecoveryEntry,
    cancellation: &ScanCancellation,
) -> Result<(), String> {
    let Some(identity) = entry.restored_identity else {
        return Err("terminal Restore receipt lacks a restored file identity".into());
    };
    let read = read_current_replace_target(
        entry,
        identity,
        entry.original_length,
        entry.restored_modified,
        entry.original_sha256,
        cancellation,
    )?;
    if read.identity() != identity || read.content_sha256() != entry.original_sha256 {
        return Err("restored source was edited or replaced after Restore".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct ReplaceWorkerSnapshot {
    pub active: bool,
    pub shutdown_requested: bool,
    pub worker_exited: bool,
    pub progress: Option<ReplaceProgress>,
    pub result: Option<ReplaceOperationResult>,
}

enum ReplaceWorkerCommand {
    Run {
        operation: ReplaceWorkerOperation,
        cancellation: ScanCancellation,
    },
    Shutdown,
}

enum ReplaceWorkerOperation {
    Apply(ConfirmedReplacementRequest),
    Restore(String),
    Recover(String),
}

struct ReplaceWorkerState {
    snapshot: ReplaceWorkerSnapshot,
    active_cancellation: Option<ScanCancellation>,
}

/// One bounded worker owner for confirmed replace and Restore operations.
/// Results remain retained until taken, preventing a second batch from being
/// admitted while its terminal receipt is still unpublished to the caller.
pub struct ReplaceExecutorWorker {
    sender: SyncSender<ReplaceWorkerCommand>,
    state: Arc<Mutex<ReplaceWorkerState>>,
    worker: Option<JoinHandle<()>>,
}

impl ReplaceExecutorWorker {
    pub fn new(executor: ReplaceExecutor) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let state = Arc::new(Mutex::new(ReplaceWorkerState {
            snapshot: ReplaceWorkerSnapshot::default(),
            active_cancellation: None,
        }));
        let worker_state = Arc::clone(&state);
        let executor = Arc::new(executor);
        let worker = thread::Builder::new()
            .name("search-replace-worker".into())
            .spawn(move || replace_worker_loop(executor, receiver, worker_state))?;
        Ok(Self {
            sender,
            state,
            worker: Some(worker),
        })
    }

    pub fn submit_apply(&self, request: ConfirmedReplacementRequest) -> std::io::Result<()> {
        self.submit(ReplaceWorkerOperation::Apply(request))
    }

    pub fn submit_restore(&self, transaction_id: impl Into<String>) -> std::io::Result<()> {
        self.submit(ReplaceWorkerOperation::Restore(transaction_id.into()))
    }

    pub fn submit_recovery(&self, transaction_id: impl Into<String>) -> std::io::Result<()> {
        self.submit(ReplaceWorkerOperation::Recover(transaction_id.into()))
    }

    fn submit(&self, operation: ReplaceWorkerOperation) -> std::io::Result<()> {
        let cancellation = ScanCancellation::new();
        {
            let mut state = self.state.lock().map_err(|_| replace_worker_poisoned())?;
            if state.snapshot.shutdown_requested {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "search and replace worker is shutting down",
                ));
            }
            if state.snapshot.active || state.snapshot.result.is_some() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "the search and replace worker is busy or has an unconsumed result",
                ));
            }
            state.snapshot.active = true;
            state.snapshot.progress = None;
            state.snapshot.result = None;
            state.active_cancellation = Some(cancellation.clone());
        }
        match self.sender.try_send(ReplaceWorkerCommand::Run {
            operation,
            cancellation,
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                if let Ok(mut state) = self.state.lock() {
                    state.snapshot.active = false;
                    state.active_cancellation = None;
                }
                Err(std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "the search and replace worker queue is full",
                ))
            }
            Err(TrySendError::Disconnected(_)) => {
                if let Ok(mut state) = self.state.lock() {
                    state.snapshot.active = false;
                    state.snapshot.worker_exited = true;
                    state.active_cancellation = None;
                }
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "the search and replace worker has exited",
                ))
            }
        }
    }

    pub fn snapshot(&self) -> ReplaceWorkerSnapshot {
        self.state
            .lock()
            .map(|state| state.snapshot.clone())
            .unwrap_or_else(|_| ReplaceWorkerSnapshot {
                worker_exited: true,
                ..ReplaceWorkerSnapshot::default()
            })
    }

    pub fn cancel_active(&self) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        let Some(cancellation) = state.active_cancellation.as_ref() else {
            return false;
        };
        cancellation.cancel();
        true
    }

    pub fn take_result(&self) -> Option<ReplaceOperationResult> {
        self.state.lock().ok()?.snapshot.result.take()
    }

    /// Signal cancellation and shutdown. Keep this owner and poll try_reap
    /// until the worker exits; native operations are never abandoned forcibly.
    pub fn request_shutdown(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.snapshot.shutdown_requested = true;
            if let Some(cancellation) = state.active_cancellation.as_ref() {
                cancellation.cancel();
            }
        }
        let _ = self.sender.try_send(ReplaceWorkerCommand::Shutdown);
    }

    pub fn try_reap(&mut self) -> std::io::Result<bool> {
        let Some(worker) = self.worker.as_ref() else {
            return Ok(true);
        };
        if !worker.is_finished() {
            return Ok(false);
        }
        let Some(worker) = self.worker.take() else {
            return Ok(true);
        };
        worker
            .join()
            .map_err(|_| std::io::Error::other("search and replace worker panicked"))?;
        if let Ok(mut state) = self.state.lock() {
            state.snapshot.worker_exited = true;
        }
        Ok(true)
    }

    /// Request shutdown and join. Use off the UI frame path.
    pub fn shutdown(mut self) -> std::io::Result<()> {
        self.request_shutdown();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| std::io::Error::other("search and replace worker panicked"))?;
        }
        if let Ok(mut state) = self.state.lock() {
            state.snapshot.worker_exited = true;
        }
        Ok(())
    }
}

impl Drop for ReplaceExecutorWorker {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

fn replace_worker_loop(
    executor: Arc<ReplaceExecutor>,
    receiver: Receiver<ReplaceWorkerCommand>,
    state: Arc<Mutex<ReplaceWorkerState>>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            ReplaceWorkerCommand::Shutdown => break,
            ReplaceWorkerCommand::Run {
                operation,
                cancellation,
            } => {
                let report_state = Arc::clone(&state);
                let report = move |progress: ReplaceProgress| {
                    if let Ok(mut state) = report_state.lock() {
                        if state.snapshot.active {
                            state.snapshot.progress = Some(ReplaceProgress {
                                message: bounded_message(&progress.message),
                                ..progress
                            });
                        }
                    }
                };
                let result = match operation {
                    ReplaceWorkerOperation::Apply(request) => {
                        executor.execute_confirmed(request, &cancellation, report)
                    }
                    ReplaceWorkerOperation::Restore(transaction_id) => {
                        executor.restore(&transaction_id, &cancellation, report)
                    }
                    ReplaceWorkerOperation::Recover(transaction_id) => {
                        executor.recover(&transaction_id, &cancellation, report)
                    }
                };
                let should_shutdown = if let Ok(mut state) = state.lock() {
                    state.snapshot.result = Some(result);
                    state.snapshot.active = false;
                    state.active_cancellation = None;
                    state.snapshot.shutdown_requested
                } else {
                    true
                };
                if should_shutdown {
                    break;
                }
            }
        }
    }
    if let Ok(mut state) = state.lock() {
        state.snapshot.active = false;
        state.snapshot.worker_exited = true;
        state.active_cancellation = None;
    }
}

fn replace_worker_poisoned() -> std::io::Error {
    std::io::Error::other("search and replace worker state is poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_test_journal() -> ReplaceRecoveryJournal {
        let transaction_id = "0123456789abcdef0123456789abcdef";
        let original = b"old";
        let output = b"new";
        let entry = ReplaceRecoveryEntry {
            root_path: std::env::current_dir().expect("absolute test root"),
            root_identity: NativeFileIdentity::default(),
            relative_path: PathBuf::from("item.txt"),
            parent_identity: NativeFileIdentity::default(),
            original_identity: NativeFileIdentity::default(),
            original_length: original.len() as u64,
            original_modified: SystemTime::UNIX_EPOCH,
            original_sha256: Sha256::digest(original).into(),
            output_length: output.len() as u64,
            output_sha256: Sha256::digest(output).into(),
            backup_leaf: role_leaf(transaction_id, 0, "backup"),
            backup_identity: None,
            output_stage_leaf: role_leaf(transaction_id, 0, "output"),
            output_stage_identity: None,
            original_stage_leaf: role_leaf(transaction_id, 0, "source"),
            original_stage_identity: None,
            post_identity: None,
            post_modified: None,
            restore_stage_leaf: role_leaf(transaction_id, 0, "restore"),
            restore_stage_identity: None,
            restore_source_stage_leaf: role_leaf(transaction_id, 0, "post"),
            restore_source_stage_identity: None,
            restored_identity: None,
            restored_modified: None,
            phase: ReplaceEntryPhase::Planned,
            outcome: ReplaceEntryOutcome::Pending,
            message: String::new(),
        };
        ReplaceRecoveryJournal {
            schema_version: REPLACE_JOURNAL_SCHEMA_VERSION,
            receipts: vec![ReplaceRecoveryReceipt {
                id: transaction_id.to_owned(),
                search_revision: 1,
                search_generation: 2,
                replacement_revision: 3,
                status: ReplaceTransactionStatus::Applying,
                entries: vec![entry],
                message: String::new(),
            }],
        }
    }

    #[test]
    fn replace_safety_boundaries_reject_untrusted_receipt_roles_and_schema() {
        let mut journal = valid_test_journal();
        assert!(validate_replace_journal(&journal).is_ok());

        journal.receipts[0].entries[0].post_modified = Some(
            SystemTime::UNIX_EPOCH
                .checked_sub(std::time::Duration::new(2, 17))
                .expect("pre-epoch timestamp is supported"),
        );
        journal.receipts[0].entries[0].restored_modified = Some(
            SystemTime::UNIX_EPOCH
                .checked_add(std::time::Duration::new(3, 29))
                .expect("post-epoch timestamp is supported"),
        );
        let expected_post_modified = journal.receipts[0].entries[0].post_modified;
        let expected_restored_modified = journal.receipts[0].entries[0].restored_modified;
        let reloaded: ReplaceRecoveryJournal =
            serde_json::from_slice(&serde_json::to_vec(&journal).expect("encode timestamps"))
                .expect("decode timestamps with the same checked representation");
        assert_eq!(
            reloaded.receipts[0].entries[0].post_modified,
            expected_post_modified
        );
        assert_eq!(
            reloaded.receipts[0].entries[0].restored_modified,
            expected_restored_modified
        );

        journal.receipts[0].entries[0].backup_leaf = "../unrelated.txt".into();
        assert!(validate_replace_journal(&journal).is_err());

        let mut encoded = serde_json::to_value(valid_test_journal()).expect("encode valid journal");
        encoded
            .as_object_mut()
            .expect("journal object")
            .insert("future_field".into(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<ReplaceRecoveryJournal>(encoded).is_err());
    }

    #[cfg(windows)]
    fn native_confirmed_candidate(root: &Path, generation: u64) -> ConfirmedReplacementRequest {
        use crate::batch_rename::ExplicitDirectoryRoot;
        use crate::search_replace::{
            ReplacementMode, ReplacementRevision, ReplacementSpec, SearchCoordinator,
            SearchFilterSpec, SearchGeneration, SearchLimits, SearchPattern, SearchPatternMode,
            SearchRevision, SearchStatus, TextSearchRequest,
        };

        let pattern = SearchPattern::compile("needle", SearchPatternMode::Literal, true)
            .expect("valid literal search pattern");
        let request = TextSearchRequest::new(
            vec![ExplicitDirectoryRoot::new(root.to_path_buf()).expect("explicit fixture root")],
            SearchFilterSpec::default(),
            pattern,
            SearchRevision(71),
            SearchGeneration(generation),
            SearchLimits::default(),
        )
        .expect("valid search request");
        let mut coordinator = SearchCoordinator::new().expect("start bounded search worker");
        let ticket = coordinator.submit(request).expect("submit A discovery");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let result = loop {
            match ticket.try_recv() {
                Ok(Some(result)) => break result,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Ok(None) => panic!("A discovery did not finish within its test deadline"),
                Err(error) => {
                    panic!("A discovery worker exited before returning a result: {error}")
                }
            }
        };
        coordinator.shutdown().expect("reap bounded search worker");
        assert_eq!(result.status(), SearchStatus::Complete);
        assert!(result.summary().match_list_complete());

        let selected = result
            .matched_files()
            .iter()
            .flat_map(|file| file.matches())
            .map(|matched| matched.id().clone())
            .collect::<Vec<_>>();
        assert!(!selected.is_empty());
        let replacement =
            ReplacementSpec::new("changed", ReplacementMode::Literal, ReplacementRevision(19))
                .compile(result.pattern())
                .expect("literal replacement compiles");
        let candidate = result
            .preview_replacement(&replacement, &selected)
            .expect("B candidate uses the completed A snapshot");
        ConfirmedReplacementRequest::confirm(candidate)
            .expect("explicit confirmation retains the native source evidence")
    }

    #[cfg(windows)]
    #[test]
    fn native_confirmed_replace_apply_restore_and_publication_collision_are_safe() {
        use tempfile::Builder;

        let fixture_parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("productivity_expansion");
        std::fs::create_dir_all(&fixture_parent).expect("create fixture parent inside target");
        let workspace = Builder::new()
            .prefix("m9c-native-transaction-")
            .tempdir_in(&fixture_parent)
            .expect("create scoped fixture");

        let root = workspace.path().join("roundtrip-root");
        let recovery = workspace.path().join("roundtrip-recovery");
        std::fs::create_dir_all(&root).expect("create roundtrip root");
        let original_a = b"\xEF\xBB\xBFneedle one\r\nlast";
        let original_b = b"needle two\r\nno-final-newline";
        std::fs::write(root.join("a.txt"), original_a).expect("create first source");
        std::fs::write(root.join("b.txt"), original_b).expect("create second source");

        let confirmed = native_confirmed_candidate(&root, 81);
        let executor = ReplaceExecutor::new(recovery.clone()).expect("appdata recovery location");
        let applied = executor.execute_confirmed(confirmed, &ScanCancellation::new(), |_| {});
        assert_eq!(
            applied.receipt.status(),
            ReplaceTransactionStatus::Applied,
            "{}",
            applied.message
        );
        assert_eq!(
            std::fs::read(root.join("a.txt")).expect("first published output"),
            b"\xEF\xBB\xBFchanged one\r\nlast"
        );
        assert_eq!(
            std::fs::read(root.join("b.txt")).expect("second published output"),
            b"changed two\r\nno-final-newline"
        );
        for entry in applied.receipt.entries() {
            assert!(entry.backup_identity.is_some());
            let expected = match entry.path().file_name().and_then(|name| name.to_str()) {
                Some("a.txt") => original_a.as_slice(),
                Some("b.txt") => original_b.as_slice(),
                _ => panic!("unexpected path in apply receipt"),
            };
            assert_eq!(
                std::fs::read(recovery.join(&entry.backup_leaf)).expect("durable original backup"),
                expected
            );
        }

        let transaction_id = applied.receipt.id().to_owned();
        let changed_post = b"external edit after apply";
        std::fs::write(root.join("b.txt"), changed_post).expect("edit post-write target");
        let restored = executor.restore(&transaction_id, &ScanCancellation::new(), |_| {});
        assert_eq!(restored.receipt.status(), ReplaceTransactionStatus::Partial);
        assert_eq!(
            std::fs::read(root.join("a.txt")).expect("original bytes restored"),
            original_a
        );
        assert_eq!(
            std::fs::read(root.join("b.txt")).expect("changed post target retained"),
            changed_post
        );
        assert!(restored.receipt.entries().iter().any(|entry| {
            entry.path().file_name().and_then(|name| name.to_str()) == Some("a.txt")
                && entry.outcome == ReplaceEntryOutcome::Restored
        }));
        assert!(restored.receipt.entries().iter().any(|entry| {
            entry.path().file_name().and_then(|name| name.to_str()) == Some("b.txt")
                && entry.outcome == ReplaceEntryOutcome::Refused
        }));

        let stream_root = workspace.path().join("stream-root");
        let stream_recovery = workspace.path().join("stream-recovery");
        std::fs::create_dir_all(&stream_root).expect("create stream root");
        let stream_source = stream_root.join("with-stream.txt");
        let stream_original = b"needle with named stream";
        let named_stream = PathBuf::from(format!("{}:m9c-metadata", stream_source.display()));
        std::fs::write(&stream_source, stream_original).expect("create stream source");
        std::fs::write(&named_stream, b"preserve this named stream")
            .expect("create NTFS named stream");
        let stream_candidate = native_confirmed_candidate(&stream_root, 83);
        let stream_executor =
            ReplaceExecutor::new(stream_recovery.clone()).expect("stream recovery location");
        let stream_result =
            stream_executor.execute_confirmed(stream_candidate, &ScanCancellation::new(), |_| {});
        assert_eq!(
            stream_result.receipt.status(),
            ReplaceTransactionStatus::Failed
        );
        assert_eq!(
            std::fs::read(&stream_source).expect("streamed source remains unchanged"),
            stream_original
        );
        assert_eq!(
            std::fs::read(&named_stream).expect("named stream remains unchanged"),
            b"preserve this named stream"
        );
        assert!(stream_result.receipt.entries()[0].backup_identity.is_none());
        assert!(
            !stream_recovery
                .join(&stream_result.receipt.entries()[0].backup_leaf)
                .exists()
        );

        let collision_root = workspace.path().join("collision-root");
        let collision_recovery = workspace.path().join("collision-recovery");
        std::fs::create_dir_all(&collision_root).expect("create collision root");
        let collision_original = b"needle race";
        let later_original = b"needle later";
        let raced_path = collision_root.join("a-raced.txt");
        let later_path = collision_root.join("z-later.txt");
        std::fs::write(&raced_path, collision_original).expect("create collision source");
        std::fs::write(&later_path, later_original).expect("create not-attempted source");
        let confirmed = native_confirmed_candidate(&collision_root, 82);
        let collision_executor =
            ReplaceExecutor::new(collision_recovery.clone()).expect("collision recovery location");
        let _operation = collision_executor
            .operation_guard("")
            .expect("serialize injected operation");
        let mut injected = false;
        let mut report = |_| {};
        let result = collision_executor.execute_confirmed_windows_with_publication_boundary(
            confirmed,
            &ScanCancellation::new(),
            &mut report,
            &mut |path| {
                if !injected && path == raced_path.as_path() {
                    std::fs::write(path, b"raced occupant")
                        .expect("create late occupant after original relocation");
                    injected = true;
                }
            },
        );
        drop(_operation);
        assert!(injected, "the hook ran after original relocation");
        assert_eq!(
            result.receipt.status(),
            ReplaceTransactionStatus::RecoveryRequired
        );
        assert_eq!(
            std::fs::read(&raced_path).expect("late occupant remains at approved path"),
            b"raced occupant"
        );
        assert_eq!(
            std::fs::read(&later_path).expect("later file was not attempted"),
            later_original
        );
        let first = result
            .receipt
            .entries()
            .iter()
            .find(|entry| {
                entry.path().file_name().and_then(|name| name.to_str()) == Some("a-raced.txt")
            })
            .expect("recovery-required entry");
        let later = result
            .receipt
            .entries()
            .iter()
            .find(|entry| {
                entry.path().file_name().and_then(|name| name.to_str()) == Some("z-later.txt")
            })
            .expect("not-attempted entry");
        assert_eq!(first.outcome, ReplaceEntryOutcome::RecoveryRequired);
        assert_eq!(later.outcome, ReplaceEntryOutcome::NotAttempted);
        assert_eq!(
            std::fs::read(collision_root.join(&first.original_stage_leaf))
                .expect("held original stage retained"),
            collision_original
        );
        assert_eq!(
            std::fs::read(collision_root.join(&first.output_stage_leaf))
                .expect("verified output stage retained"),
            b"changed race"
        );
        assert_eq!(
            std::fs::read(collision_recovery.join(&first.backup_leaf))
                .expect("durable backup retained"),
            collision_original
        );

        let recovery_id = result.receipt.id().to_owned();
        let blocked = collision_executor.recover(&recovery_id, &ScanCancellation::new(), |_| {});
        assert_eq!(
            blocked.receipt.status(),
            ReplaceTransactionStatus::RecoveryRequired
        );
        assert_eq!(
            std::fs::read(&raced_path).expect("recovery preserves a late occupant"),
            b"raced occupant"
        );
        std::fs::remove_file(&raced_path).expect("remove test-owned late occupant");
        let recovered = collision_executor.recover(&recovery_id, &ScanCancellation::new(), |_| {});
        assert_eq!(
            recovered.receipt.status(),
            ReplaceTransactionStatus::Partial
        );
        assert_eq!(
            std::fs::read(&raced_path).expect("recorded output stage recovered"),
            b"changed race"
        );
        assert_eq!(
            std::fs::read(&later_path).expect("later file remains not attempted"),
            later_original
        );
    }
}
