//! Confirmed batch-rename execution, recovery journal and safe undo.
//!
//! This module owns transaction phases and their durable receipts. Native
//! source handles and no-replace publication stay inside `file_operations`.

use crate::batch_rename;
use crate::common::persistence::save_json_atomic;
use crate::file_operations::{
    self, ContentVerificationMode, NativeFileIdentity, ScanCancellation, UndoGuardSpec,
    VerifiedRenamePlan,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const RECOVERY_JOURNAL_FILE: &str = "batch_rename_recovery.json";
pub const MAX_RETAINED_RENAME_RECEIPTS: usize = 32;
const MAX_STAGE_ATTEMPTS: usize = 64;
const MAX_JOURNAL_BYTES: usize = 16 * 1024 * 1024;
const JOURNAL_SCHEMA_VERSION: u32 = 2;

mod system_time_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EncodedSystemTime {
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
        let value = EncodedSystemTime::deserialize(deserializer)?;
        if value.nanoseconds >= 1_000_000_000 {
            return Err(D::Error::custom("invalid SystemTime nanoseconds"));
        }
        let duration = Duration::new(value.seconds, value.nanoseconds);
        if value.before_unix_epoch {
            UNIX_EPOCH
                .checked_sub(duration)
                .ok_or_else(|| D::Error::custom("SystemTime is outside supported range"))
        } else {
            UNIX_EPOCH
                .checked_add(duration)
                .ok_or_else(|| D::Error::custom("SystemTime is outside supported range"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenameTransactionStatus {
    Applying,
    Applied,
    Undoing,
    Undone,
    Restored,
    Partial,
    RecoveryRequired,
    Failed,
    Cancelled,
    Refused,
}

impl RenameTransactionStatus {
    pub fn can_undo(self) -> bool {
        matches!(
            self,
            Self::Applied | Self::Partial | Self::RecoveryRequired | Self::Undoing
        )
    }

    pub(crate) fn blocks_new_apply(self) -> bool {
        matches!(
            self,
            Self::RecoveryRequired | Self::Applying | Self::Undoing
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenameEntryOutcome {
    Pending,
    Staged,
    Applied,
    Restored,
    Refused,
    Failed,
    Cancelled,
    Retained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenameTransactionPhase {
    Preparing,
    Staging,
    Publishing,
    RollingBack,
    Undoing,
    Complete,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameProgress {
    pub transaction_id: String,
    pub phase: RenameTransactionPhase,
    pub completed: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryJournal {
    schema_version: u32,
    receipts: Vec<RenameRecoveryReceipt>,
}

impl Default for RecoveryJournal {
    fn default() -> Self {
        Self {
            schema_version: JOURNAL_SCHEMA_VERSION,
            receipts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameRecoveryReceipt {
    id: String,
    root_path: PathBuf,
    root_identity: NativeFileIdentity,
    generation: u64,
    rules_revision: u64,
    status: RenameTransactionStatus,
    #[serde(default)]
    undo_previous_status: Option<RenameTransactionStatus>,
    entries: Vec<RenameRecoveryEntry>,
    pending_move: Option<PendingMove>,
    message: String,
}

impl RenameRecoveryReceipt {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn status(&self) -> RenameTransactionStatus {
        self.status
    }

    pub fn entries(&self) -> &[RenameRecoveryEntry] {
        &self.entries
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn remaining_undo_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| {
                entry
                    .current_path
                    .as_ref()
                    .is_some_and(|current| current != &entry.original_path)
            })
            .count()
    }

    pub fn is_recovery_required(&self) -> bool {
        self.status == RenameTransactionStatus::RecoveryRequired
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameRecoveryEntry {
    source_ref: u64,
    original_path: PathBuf,
    stage_path: PathBuf,
    /// Staging leaves recorded before their atomic move. Keeping the prior
    /// owned stage alongside the pending candidate makes the crash gap
    /// reconcilable without trusting an arbitrary same-parent path.
    stage_paths: Vec<PathBuf>,
    final_path: PathBuf,
    parent_path: PathBuf,
    parent_identity: NativeFileIdentity,
    identity: NativeFileIdentity,
    expected_length: u64,
    #[serde(with = "system_time_serde")]
    expected_modified: SystemTime,
    content_sha256: [u8; 32],
    current_path: Option<PathBuf>,
    #[serde(default)]
    undo_restore_path: Option<PathBuf>,
    outcome: RenameEntryOutcome,
}

impl RenameRecoveryEntry {
    pub fn original_path(&self) -> &Path {
        &self.original_path
    }

    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    pub fn outcome(&self) -> RenameEntryOutcome {
        self.outcome
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingMove {
    source_ref: u64,
    from: PathBuf,
    to: PathBuf,
    identity: NativeFileIdentity,
    content_sha256: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct RenameOperationResult {
    pub receipt: RenameRecoveryReceipt,
    pub message: String,
}

/// Narrow seam around a preflighted rename file. Production operations keep
/// their native handle implementation in `file_operations`; tests can model
/// occupied names and interrupted moves without replacing that boundary.
trait GuardedRenameFileOps {
    fn source_ref(&self) -> batch_rename::SnapshotEntryRef;
    fn original_path(&self) -> &Path;
    fn target_path(&self) -> &Path;
    fn current_path(&self) -> &Path;
    fn parent_path(&self) -> &Path;
    fn parent_identity(&self) -> NativeFileIdentity;
    fn identity(&self) -> NativeFileIdentity;
    fn expected_length(&self) -> u64;
    fn expected_modified(&self) -> SystemTime;
    fn content_sha256(&self) -> [u8; 32];
    fn rename_to_leaf(
        &mut self,
        leaf: &std::ffi::OsStr,
        verification_mode: ContentVerificationMode<'_>,
    ) -> io::Result<PathBuf>;
    fn verify_at(
        &self,
        expected_path: &Path,
        verification_mode: ContentVerificationMode<'_>,
    ) -> io::Result<()>;
}

trait GuardedRenameBatchOps {
    type File: GuardedRenameFileOps;

    fn files(&self) -> &[Self::File];
    fn files_mut(&mut self) -> &mut [Self::File];
}

impl GuardedRenameFileOps for file_operations::GuardedRenameFile {
    fn source_ref(&self) -> batch_rename::SnapshotEntryRef {
        self.source_ref()
    }

    fn original_path(&self) -> &Path {
        self.original_path()
    }

    fn target_path(&self) -> &Path {
        self.target_path()
    }

    fn current_path(&self) -> &Path {
        self.current_path()
    }

    fn parent_path(&self) -> &Path {
        self.parent_path()
    }

    fn parent_identity(&self) -> NativeFileIdentity {
        self.parent_identity()
    }

    fn identity(&self) -> NativeFileIdentity {
        self.identity()
    }

    fn expected_length(&self) -> u64 {
        self.expected_length()
    }

    fn expected_modified(&self) -> SystemTime {
        self.expected_modified()
    }

    fn content_sha256(&self) -> [u8; 32] {
        self.content_sha256()
    }

    fn rename_to_leaf(
        &mut self,
        leaf: &std::ffi::OsStr,
        verification_mode: ContentVerificationMode<'_>,
    ) -> io::Result<PathBuf> {
        self.rename_to_leaf(leaf, verification_mode)
    }

    fn verify_at(
        &self,
        expected_path: &Path,
        verification_mode: ContentVerificationMode<'_>,
    ) -> io::Result<()> {
        self.verify_at(expected_path, verification_mode)
    }
}

impl GuardedRenameBatchOps for file_operations::GuardedRenameBatch {
    type File = file_operations::GuardedRenameFile;

    fn files(&self) -> &[Self::File] {
        &self.files
    }

    fn files_mut(&mut self) -> &mut [Self::File] {
        &mut self.files
    }
}

/// Load the application-owned journal on the worker and verify only its
/// recorded root, parent and candidate paths. This never scans a folder or
/// changes a user file.
pub(crate) fn load_and_reconcile_journal(
    journal_path: &Path,
    cancellation: &ScanCancellation,
) -> Result<Vec<RenameRecoveryReceipt>, String> {
    let mut journal = load_journal(journal_path)?;
    for receipt in &mut journal.receipts {
        if cancellation.is_cancelled() {
            return Err("rename recovery verification was cancelled".to_owned());
        }
        validate_receipt(receipt)?;
        reconcile_receipt(receipt, cancellation)?;
    }
    if cancellation.is_cancelled() {
        return Err("rename recovery verification was cancelled".to_owned());
    }
    save_journal(journal_path, &journal)?;
    Ok(journal.receipts)
}

pub(crate) fn execute_apply(
    plan: VerifiedRenamePlan,
    journal_path: &Path,
    cancellation: &ScanCancellation,
    mut report: impl FnMut(RenameProgress),
) -> RenameOperationResult {
    let transaction_id = format!("{:032x}", rand::random::<u128>());
    report(RenameProgress {
        transaction_id: transaction_id.clone(),
        phase: RenameTransactionPhase::Preparing,
        completed: 0,
        total: plan.plan().actions().len(),
        message: "Rechecking the confirmed files and destinations".to_owned(),
    });
    let mut journal = match load_journal(journal_path) {
        Ok(journal) => journal,
        Err(error) => return refused_result(transaction_id, error),
    };
    if journal
        .receipts
        .iter()
        .any(|receipt| receipt.status.blocks_new_apply())
    {
        return refused_result(
            transaction_id,
            "an unresolved rename recovery receipt must be handled first".to_owned(),
        );
    }
    journal
        .receipts
        .retain(|receipt| receipt.status != RenameTransactionStatus::Undone);
    if journal.receipts.len() >= MAX_RETAINED_RENAME_RECEIPTS {
        return refused_result(
            transaction_id,
            "the rename recovery journal is full; undo an earlier batch before applying another"
                .to_owned(),
        );
    }
    if cancellation.is_cancelled() {
        return refused_result(
            transaction_id,
            "rename was cancelled before any file was moved".to_owned(),
        );
    }
    let mut batch = match file_operations::prepare_guarded_rename_batch(&plan, cancellation) {
        Ok(batch) => batch,
        Err(error) => return refused_result(transaction_id, error),
    };
    if cancellation.is_cancelled() {
        return refused_result(
            transaction_id,
            "rename was cancelled before any file was moved".to_owned(),
        );
    }

    let mut receipt = receipt_from_plan(&transaction_id, &plan, &batch);
    journal.receipts.push(receipt.clone());
    if let Err(error) = save_journal(journal_path, &journal) {
        receipt.status = RenameTransactionStatus::Failed;
        receipt.message =
            format!("recovery journal could not be written before any files changed: {error}");
        return RenameOperationResult {
            message: receipt.message.clone(),
            receipt,
        };
    }

    for index in 0..batch.files.len() {
        if cancellation.is_cancelled() {
            return rollback_apply(
                &mut journal,
                &mut receipt,
                &mut batch,
                journal_path,
                &transaction_id,
                "rename was cancelled",
                true,
                &mut report,
            );
        }
        report(RenameProgress {
            transaction_id: transaction_id.clone(),
            phase: RenameTransactionPhase::Staging,
            completed: index,
            total: batch.files.len(),
            message: format!("Staging {}", batch.files[index].original_path().display()),
        });
        match stage_entry(
            &mut journal,
            &mut receipt,
            &mut batch,
            index,
            journal_path,
            &transaction_id,
            ContentVerificationMode::Cooperative(cancellation),
        ) {
            Ok(()) => {}
            Err(StepFailure::Mutation(error)) => {
                return rollback_apply(
                    &mut journal,
                    &mut receipt,
                    &mut batch,
                    journal_path,
                    &transaction_id,
                    &error,
                    false,
                    &mut report,
                );
            }
            Err(error @ (StepFailure::Journal(_) | StepFailure::Uncertain(_))) => {
                return stop_for_recovery(
                    &mut journal,
                    &mut receipt,
                    &transaction_id,
                    error.to_string(),
                );
            }
        }
    }

    for index in 0..batch.files.len() {
        if cancellation.is_cancelled() {
            return rollback_apply(
                &mut journal,
                &mut receipt,
                &mut batch,
                journal_path,
                &transaction_id,
                "rename was cancelled",
                true,
                &mut report,
            );
        }
        report(RenameProgress {
            transaction_id: transaction_id.clone(),
            phase: RenameTransactionPhase::Publishing,
            completed: index,
            total: batch.files.len(),
            message: format!("Publishing {}", batch.files[index].target_path().display()),
        });
        if let Err(error) = publish_entry(
            &mut journal,
            &mut receipt,
            &mut batch,
            index,
            journal_path,
            &transaction_id,
            ContentVerificationMode::Cooperative(cancellation),
        ) {
            match error {
                StepFailure::Mutation(message) => {
                    return rollback_apply(
                        &mut journal,
                        &mut receipt,
                        &mut batch,
                        journal_path,
                        &transaction_id,
                        &message,
                        false,
                        &mut report,
                    );
                }
                error @ (StepFailure::Journal(_) | StepFailure::Uncertain(_)) => {
                    return stop_for_recovery(
                        &mut journal,
                        &mut receipt,
                        &transaction_id,
                        error.to_string(),
                    );
                }
            }
        }
    }
    receipt.status = RenameTransactionStatus::Applied;
    receipt.message = "All confirmed filenames were applied and verified".to_owned();
    receipt.pending_move = None;
    if let Some(stored) = find_receipt_mut(&mut journal, &transaction_id) {
        *stored = receipt.clone();
    }
    if let Err(error) = save_journal(journal_path, &journal) {
        receipt.status = RenameTransactionStatus::RecoveryRequired;
        receipt.message = format!(
            "files were renamed, but the final recovery receipt could not be saved: {error}"
        );
    }
    report(RenameProgress {
        transaction_id: transaction_id.clone(),
        phase: RenameTransactionPhase::Complete,
        completed: batch.files.len(),
        total: batch.files().len(),
        message: receipt.message.clone(),
    });
    RenameOperationResult {
        message: receipt.message.clone(),
        receipt,
    }
}

pub(crate) fn execute_undo(
    transaction_id: &str,
    journal_path: &Path,
    cancellation: &ScanCancellation,
    mut report: impl FnMut(RenameProgress),
) -> RenameOperationResult {
    let mut journal = match load_and_reconcile_journal(journal_path, cancellation) {
        Ok(receipts) => RecoveryJournal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            receipts,
        },
        Err(error) => return refused_result(transaction_id.to_owned(), error),
    };
    let Some(index) = journal
        .receipts
        .iter()
        .position(|receipt| receipt.id == transaction_id)
    else {
        return refused_result(
            transaction_id.to_owned(),
            "rename receipt was not found".to_owned(),
        );
    };
    let mut receipt = journal.receipts[index].clone();
    if !receipt.status.can_undo() {
        let status = receipt.status;
        return refused_receipt(
            receipt,
            format!("receipt state {status:?} cannot be undone"),
        );
    }
    if cancellation.is_cancelled() {
        return refused_result(
            transaction_id.to_owned(),
            "Undo was cancelled before any file was moved".to_owned(),
        );
    }
    let specs = match undo_specs(&receipt) {
        Ok(specs) => specs,
        Err(error) => return refused_receipt(receipt, error),
    };
    if specs.is_empty() {
        receipt.status = RenameTransactionStatus::Undone;
        receipt.message =
            "All original filenames already contain their recorded identities and bytes".to_owned();
        journal.receipts[index] = receipt.clone();
        if let Err(error) = save_journal(journal_path, &journal) {
            receipt.status = RenameTransactionStatus::RecoveryRequired;
            receipt.message = format!("the verified Undo state could not be saved: {error}");
        }
        return RenameOperationResult {
            message: receipt.message.clone(),
            receipt,
        };
    }
    let mut batch = match file_operations::prepare_guarded_undo_batch(
        &receipt.root_path,
        receipt.root_identity,
        &specs,
        cancellation,
    ) {
        Ok(batch) => batch,
        Err(error) => return refused_receipt(receipt, error),
    };
    let prior_status = receipt.status;
    for entry in &mut receipt.entries {
        entry.undo_restore_path = entry.current_path.clone();
    }
    receipt.status = RenameTransactionStatus::Undoing;
    receipt.undo_previous_status = Some(prior_status);
    receipt.message = "Undo is in progress".to_owned();
    journal.receipts[index] = receipt.clone();
    if let Err(error) = save_journal(journal_path, &journal) {
        receipt.status = prior_status;
        receipt.undo_previous_status = None;
        for entry in &mut receipt.entries {
            entry.undo_restore_path = None;
        }
        receipt.message = format!(
            "Undo was not started because its recovery state could not be written: {error}"
        );
        return RenameOperationResult {
            message: receipt.message.clone(),
            receipt,
        };
    }
    let total = batch.files.len();
    for position in 0..total {
        if cancellation.is_cancelled() {
            return rollback_undo(
                &mut journal,
                &mut receipt,
                &mut batch,
                journal_path,
                transaction_id,
                "Undo was cancelled",
                true,
                prior_status,
                &mut report,
            );
        }
        report(RenameProgress {
            transaction_id: transaction_id.to_owned(),
            phase: RenameTransactionPhase::Undoing,
            completed: position,
            total,
            message: format!(
                "Restoring {}",
                batch.files[position].original_path().display()
            ),
        });
        let already_at_original = path_matches(
            batch.files[position].current_path(),
            batch.files[position].original_path(),
        );
        let already_staged = receipt
            .entries
            .iter()
            .find(|entry| entry.source_ref == batch.files[position].source_ref().0)
            .is_some_and(|entry| {
                path_matches(batch.files[position].current_path(), &entry.stage_path)
            });
        if already_at_original || already_staged {
            continue;
        }
        if let Err(error) = stage_entry(
            &mut journal,
            &mut receipt,
            &mut batch,
            position,
            journal_path,
            transaction_id,
            ContentVerificationMode::Cooperative(cancellation),
        ) {
            match error {
                StepFailure::Mutation(message) => {
                    return rollback_undo(
                        &mut journal,
                        &mut receipt,
                        &mut batch,
                        journal_path,
                        transaction_id,
                        &message,
                        false,
                        prior_status,
                        &mut report,
                    );
                }
                error @ (StepFailure::Journal(_) | StepFailure::Uncertain(_)) => {
                    return stop_for_recovery(
                        &mut journal,
                        &mut receipt,
                        transaction_id,
                        error.to_string(),
                    );
                }
            }
        }
    }
    for position in 0..total {
        if cancellation.is_cancelled() {
            return rollback_undo(
                &mut journal,
                &mut receipt,
                &mut batch,
                journal_path,
                transaction_id,
                "Undo was cancelled",
                true,
                prior_status,
                &mut report,
            );
        }
        if path_matches(
            batch.files[position].current_path(),
            batch.files[position].original_path(),
        ) {
            continue;
        }
        if let Err(error) = publish_entry(
            &mut journal,
            &mut receipt,
            &mut batch,
            position,
            journal_path,
            transaction_id,
            ContentVerificationMode::Cooperative(cancellation),
        ) {
            match error {
                StepFailure::Mutation(message) => {
                    return rollback_undo(
                        &mut journal,
                        &mut receipt,
                        &mut batch,
                        journal_path,
                        transaction_id,
                        &message,
                        false,
                        prior_status,
                        &mut report,
                    );
                }
                error @ (StepFailure::Journal(_) | StepFailure::Uncertain(_)) => {
                    return stop_for_recovery(
                        &mut journal,
                        &mut receipt,
                        transaction_id,
                        error.to_string(),
                    );
                }
            }
        }
    }
    if batch.files.iter().any(|file| {
        file.verify_at(
            file.original_path(),
            ContentVerificationMode::CompleteOwnedPayload,
        )
        .is_err()
            || file.current_path() != file.original_path()
    }) {
        receipt.status = RenameTransactionStatus::RecoveryRequired;
        receipt.message =
            "Undo could not verify every original identity and byte fingerprint".to_owned();
        journal.receipts[index] = receipt.clone();
        let _ = save_journal(journal_path, &journal);
        return RenameOperationResult {
            message: receipt.message.clone(),
            receipt,
        };
    }
    for entry in &mut receipt.entries {
        entry.current_path = Some(entry.original_path.clone());
        entry.undo_restore_path = None;
        entry.outcome = RenameEntryOutcome::Restored;
        normalize_recorded_stage_paths(entry, None);
    }
    receipt.status = RenameTransactionStatus::Undone;
    receipt.undo_previous_status = None;
    receipt.message = "Undo restored and verified all original identities and bytes".to_owned();
    receipt.pending_move = None;
    journal.receipts[index] = receipt.clone();
    if let Err(error) = save_journal(journal_path, &journal) {
        receipt.status = RenameTransactionStatus::RecoveryRequired;
        receipt.message = format!("Undo completed, but its receipt could not be saved: {error}");
    }
    report(RenameProgress {
        transaction_id: transaction_id.to_owned(),
        phase: RenameTransactionPhase::Complete,
        completed: total,
        total,
        message: receipt.message.clone(),
    });
    RenameOperationResult {
        message: receipt.message.clone(),
        receipt,
    }
}

#[derive(Debug)]
enum StepFailure {
    Journal(String),
    Mutation(String),
    Uncertain(String),
}

impl std::fmt::Display for StepFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Journal(message) => {
                write!(formatter, "recovery journal update failed: {message}")
            }
            Self::Mutation(message) | Self::Uncertain(message) => formatter.write_str(message),
        }
    }
}

fn stage_entry<B: GuardedRenameBatchOps>(
    journal: &mut RecoveryJournal,
    receipt: &mut RenameRecoveryReceipt,
    batch: &mut B,
    index: usize,
    journal_path: &Path,
    transaction_id: &str,
    verification_mode: ContentVerificationMode<'_>,
) -> Result<(), StepFailure> {
    let file = &mut batch.files_mut()[index];
    let entry_index = receipt
        .entries
        .iter()
        .position(|entry| entry.source_ref == file.source_ref().0)
        .ok_or_else(|| {
            StepFailure::Uncertain("the operation file is missing from its receipt".to_owned())
        })?;
    for _ in 0..MAX_STAGE_ATTEMPTS {
        if verification_mode.is_cancelled() {
            return Err(StepFailure::Mutation(
                "rename was cancelled before the next atomic move".to_owned(),
            ));
        }
        let leaf = staging_leaf();
        let destination = file.parent_path().join(&leaf);
        if paths_match_windows(&destination, file.current_path()) {
            continue;
        }
        let entry = &mut receipt.entries[entry_index];
        let prior_stage = entry.stage_path.clone();
        let current_stage =
            is_staging_path(file.current_path()).then(|| file.current_path().to_path_buf());
        let restore_stage = entry
            .undo_restore_path
            .as_ref()
            .filter(|path| is_staging_path(path))
            .cloned();
        entry.stage_path = current_stage
            .clone()
            .or_else(|| restore_stage.clone())
            .unwrap_or(prior_stage);
        entry.stage_paths = vec![entry.stage_path.clone()];
        for recorded_stage in [current_stage, restore_stage].into_iter().flatten() {
            if !entry
                .stage_paths
                .iter()
                .any(|path| paths_match_windows(path, &recorded_stage))
            {
                entry.stage_paths.push(recorded_stage);
            }
        }
        if !entry
            .stage_paths
            .iter()
            .any(|path| paths_match_windows(path, &destination))
        {
            entry.stage_paths.push(destination.clone());
        }
        receipt.pending_move = Some(PendingMove {
            source_ref: file.source_ref().0,
            from: file.current_path().to_path_buf(),
            to: destination.clone(),
            identity: file.identity(),
            content_sha256: file.content_sha256(),
        });
        update_receipt(journal, receipt, journal_path).map_err(StepFailure::Journal)?;
        match file.rename_to_leaf(&leaf, verification_mode) {
            Ok(actual) => {
                let entry = &mut receipt.entries[entry_index];
                entry.stage_path = actual.clone();
                let retained_restore_stage = entry
                    .undo_restore_path
                    .as_ref()
                    .filter(|path| is_staging_path(path))
                    .cloned();
                entry.stage_paths.clear();
                entry.stage_paths.push(actual.clone());
                if let Some(restore_stage) = retained_restore_stage
                    && !paths_match_windows(&restore_stage, &actual)
                {
                    entry.stage_paths.push(restore_stage);
                }
                receipt.entries[entry_index].current_path = Some(actual);
                receipt.entries[entry_index].outcome = RenameEntryOutcome::Staged;
                receipt.pending_move = None;
                update_receipt(journal, receipt, journal_path).map_err(StepFailure::Journal)?;
                return Ok(());
            }
            Err(error) if is_destination_collision(&error) => {
                let current = file.current_path().to_path_buf();
                receipt.entries[entry_index].current_path = Some(current);
                receipt.entries[entry_index]
                    .stage_paths
                    .retain(|path| !paths_match_windows(path, &destination));
                receipt.pending_move = None;
                update_receipt(journal, receipt, journal_path).map_err(StepFailure::Journal)?;
            }
            Err(error) => {
                let current = file.current_path().to_path_buf();
                if file
                    .verify_at(&current, ContentVerificationMode::CompleteOwnedPayload)
                    .is_err()
                {
                    return Err(StepFailure::Uncertain(format!(
                        "could not verify {} after a staging error; its recorded move remains pending: {error}",
                        file.original_path().display()
                    )));
                }
                let entry = &mut receipt.entries[entry_index];
                entry.current_path = Some(current.clone());
                if paths_match_windows(&current, &destination) {
                    entry.stage_path = current.clone();
                    let restore_stage = entry
                        .undo_restore_path
                        .as_ref()
                        .filter(|path| is_staging_path(path))
                        .filter(|path| !paths_match_windows(path, &current))
                        .cloned();
                    entry.stage_paths = vec![current];
                    if let Some(restore_stage) = restore_stage {
                        entry.stage_paths.push(restore_stage);
                    }
                    entry.outcome = RenameEntryOutcome::Staged;
                } else {
                    entry
                        .stage_paths
                        .retain(|path| !paths_match_windows(path, &destination));
                    if !entry
                        .stage_paths
                        .iter()
                        .any(|path| paths_match_windows(path, &entry.stage_path))
                    {
                        entry.stage_paths.push(entry.stage_path.clone());
                    }
                }
                receipt.pending_move = None;
                update_receipt(journal, receipt, journal_path).map_err(StepFailure::Journal)?;
                return Err(StepFailure::Mutation(format!(
                    "could not stage {}: {error}",
                    file.original_path().display()
                )));
            }
        }
    }
    let _ = transaction_id;
    Err(StepFailure::Mutation(format!(
        "could not find an unoccupied staging name for {} after {MAX_STAGE_ATTEMPTS} attempts",
        batch.files()[index].original_path().display()
    )))
}

fn publish_entry<B: GuardedRenameBatchOps>(
    journal: &mut RecoveryJournal,
    receipt: &mut RenameRecoveryReceipt,
    batch: &mut B,
    index: usize,
    journal_path: &Path,
    _transaction_id: &str,
    verification_mode: ContentVerificationMode<'_>,
) -> Result<(), StepFailure> {
    let target = batch.files()[index].target_path().to_path_buf();
    publish_to_path(
        journal,
        receipt,
        batch,
        index,
        journal_path,
        &target,
        verification_mode,
    )
}

fn publish_to_path<B: GuardedRenameBatchOps>(
    journal: &mut RecoveryJournal,
    receipt: &mut RenameRecoveryReceipt,
    batch: &mut B,
    index: usize,
    journal_path: &Path,
    target_path: &Path,
    verification_mode: ContentVerificationMode<'_>,
) -> Result<(), StepFailure> {
    let file = &mut batch.files_mut()[index];
    let entry_index = receipt
        .entries
        .iter()
        .position(|entry| entry.source_ref == file.source_ref().0)
        .ok_or_else(|| {
            StepFailure::Uncertain("the operation file is missing from its receipt".to_owned())
        })?;
    let leaf = target_path
        .file_name()
        .ok_or_else(|| StepFailure::Uncertain("the confirmed target has no filename".to_owned()))?
        .to_os_string();
    receipt.pending_move = Some(PendingMove {
        source_ref: file.source_ref().0,
        from: file.current_path().to_path_buf(),
        to: target_path.to_path_buf(),
        identity: file.identity(),
        content_sha256: file.content_sha256(),
    });
    update_receipt(journal, receipt, journal_path).map_err(StepFailure::Journal)?;
    match file.rename_to_leaf(&leaf, verification_mode) {
        Ok(actual) => {
            receipt.entries[entry_index].current_path = Some(actual);
            receipt.entries[entry_index].outcome = if target_path == file.original_path() {
                RenameEntryOutcome::Restored
            } else {
                RenameEntryOutcome::Applied
            };
            receipt.pending_move = None;
            update_receipt(journal, receipt, journal_path).map_err(StepFailure::Journal)
        }
        Err(error) => {
            receipt.entries[entry_index].current_path = Some(file.current_path().to_path_buf());
            if file
                .verify_at(
                    file.current_path(),
                    ContentVerificationMode::CompleteOwnedPayload,
                )
                .is_err()
            {
                return Err(StepFailure::Uncertain(format!(
                    "could not verify {} after a publication error; its recorded move remains pending: {error}",
                    file.original_path().display()
                )));
            }
            receipt.pending_move = None;
            update_receipt(journal, receipt, journal_path).map_err(StepFailure::Journal)?;
            Err(StepFailure::Mutation(format!(
                "could not publish {} as {}: {error}",
                file.original_path().display(),
                target_path.display()
            )))
        }
    }
}

fn rollback_apply<B: GuardedRenameBatchOps>(
    journal: &mut RecoveryJournal,
    receipt: &mut RenameRecoveryReceipt,
    batch: &mut B,
    journal_path: &Path,
    transaction_id: &str,
    cause: &str,
    cancelled: bool,
    report: &mut impl FnMut(RenameProgress),
) -> RenameOperationResult {
    let restore_paths = batch
        .files()
        .iter()
        .map(|file| file.original_path().to_path_buf())
        .collect::<Vec<_>>();
    rollback_to_paths(
        journal,
        receipt,
        batch,
        journal_path,
        transaction_id,
        cause,
        &restore_paths,
        if cancelled {
            RenameTransactionStatus::Cancelled
        } else {
            RenameTransactionStatus::Restored
        },
        if cancelled {
            RenameEntryOutcome::Cancelled
        } else {
            RenameEntryOutcome::Restored
        },
        report,
    )
}

fn rollback_undo<B: GuardedRenameBatchOps>(
    journal: &mut RecoveryJournal,
    receipt: &mut RenameRecoveryReceipt,
    batch: &mut B,
    journal_path: &Path,
    transaction_id: &str,
    cause: &str,
    _cancelled: bool,
    prior_status: RenameTransactionStatus,
    report: &mut impl FnMut(RenameProgress),
) -> RenameOperationResult {
    let restore_paths = batch
        .files()
        .iter()
        .map(|file| {
            receipt
                .entries
                .iter()
                .find(|entry| entry.source_ref == file.source_ref().0)
                .and_then(|entry| entry.undo_restore_path.clone())
                .ok_or_else(|| {
                    format!(
                        "Undo recovery is missing its recorded pre-Undo path for {}",
                        file.original_path().display()
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>();
    let restore_paths = match restore_paths {
        Ok(paths) => paths,
        Err(error) => {
            return stop_for_recovery(journal, receipt, transaction_id, error);
        }
    };
    rollback_to_paths(
        journal,
        receipt,
        batch,
        journal_path,
        transaction_id,
        cause,
        &restore_paths,
        prior_status,
        RenameEntryOutcome::Applied,
        report,
    )
}

fn rollback_to_paths<B: GuardedRenameBatchOps>(
    journal: &mut RecoveryJournal,
    receipt: &mut RenameRecoveryReceipt,
    batch: &mut B,
    journal_path: &Path,
    transaction_id: &str,
    cause: &str,
    restore_paths: &[PathBuf],
    success_status: RenameTransactionStatus,
    success_outcome: RenameEntryOutcome,
    report: &mut impl FnMut(RenameProgress),
) -> RenameOperationResult {
    report(RenameProgress {
        transaction_id: transaction_id.to_owned(),
        phase: RenameTransactionPhase::RollingBack,
        completed: 0,
        total: batch.files().len(),
        message: "Restoring files to their last verified names".to_owned(),
    });
    let mut rollback_error = None;
    let mut stop = false;
    // Stage all transaction members first so cycle destinations can be vacated.
    for (index, restore_path) in restore_paths.iter().enumerate() {
        if path_matches(batch.files()[index].current_path(), restore_path) {
            continue;
        }
        let already_staged = receipt
            .entries
            .iter()
            .find(|entry| entry.source_ref == batch.files()[index].source_ref().0)
            .is_some_and(|entry| {
                path_matches(batch.files()[index].current_path(), &entry.stage_path)
            });
        if already_staged {
            continue;
        }
        match stage_entry(
            journal,
            receipt,
            batch,
            index,
            journal_path,
            transaction_id,
            ContentVerificationMode::CompleteOwnedPayload,
        ) {
            Ok(()) => {}
            Err(error @ (StepFailure::Journal(_) | StepFailure::Uncertain(_))) => {
                rollback_error.get_or_insert(error.to_string());
                stop = true;
                break;
            }
            Err(StepFailure::Mutation(error)) => {
                rollback_error.get_or_insert(error);
            }
        }
    }
    if !stop {
        for (index, restore_path) in restore_paths.iter().enumerate() {
            if path_matches(batch.files()[index].current_path(), restore_path) {
                continue;
            }
            match publish_to_path(
                journal,
                receipt,
                batch,
                index,
                journal_path,
                restore_path,
                ContentVerificationMode::CompleteOwnedPayload,
            ) {
                Ok(()) => {}
                Err(error @ (StepFailure::Journal(_) | StepFailure::Uncertain(_))) => {
                    rollback_error.get_or_insert(error.to_string());
                    break;
                }
                Err(StepFailure::Mutation(error)) => {
                    rollback_error.get_or_insert(error);
                }
            }
        }
    }
    let restored = !stop
        && batch
            .files()
            .iter()
            .zip(restore_paths)
            .all(|(file, target)| {
                path_matches(file.current_path(), target)
                    && file
                        .verify_at(target, ContentVerificationMode::CompleteOwnedPayload)
                        .is_ok()
            });
    receipt.status = if restored {
        success_status
    } else {
        RenameTransactionStatus::RecoveryRequired
    };
    if restored {
        for file in batch.files() {
            if let Some(entry) = receipt
                .entries
                .iter_mut()
                .find(|entry| entry.source_ref == file.source_ref().0)
            {
                entry.current_path = Some(file.current_path().to_path_buf());
                entry.outcome = success_outcome;
                entry.undo_restore_path = None;
                normalize_recorded_stage_paths(entry, None);
            }
        }
        receipt.pending_move = None;
        receipt.undo_previous_status = None;
    }
    receipt.message = if restored {
        format!("{cause}; all completed moves were verified at their prior names")
    } else {
        format!(
            "{cause}; recovery is required and recorded files remain at their known paths{}",
            rollback_error
                .as_ref()
                .map(|error| format!(": {error}"))
                .unwrap_or_default()
        )
    };
    if let Err(error) = update_receipt(journal, receipt, journal_path) {
        receipt.status = RenameTransactionStatus::RecoveryRequired;
        receipt.message = format!(
            "{}; its final recovery state could not be saved: {error}",
            receipt.message
        );
    }
    let phase = if receipt.status == RenameTransactionStatus::RecoveryRequired {
        RenameTransactionPhase::RecoveryRequired
    } else {
        RenameTransactionPhase::Complete
    };
    report(RenameProgress {
        transaction_id: transaction_id.to_owned(),
        phase,
        completed: batch.files().len(),
        total: batch.files().len(),
        message: receipt.message.clone(),
    });
    RenameOperationResult {
        message: receipt.message.clone(),
        receipt: receipt.clone(),
    }
}

fn stop_for_recovery(
    journal: &mut RecoveryJournal,
    receipt: &mut RenameRecoveryReceipt,
    transaction_id: &str,
    message: String,
) -> RenameOperationResult {
    receipt.status = RenameTransactionStatus::RecoveryRequired;
    receipt.message = message;
    if let Some(stored) = find_receipt_mut(journal, transaction_id) {
        *stored = receipt.clone();
    }
    RenameOperationResult {
        message: receipt.message.clone(),
        receipt: receipt.clone(),
    }
}

fn path_matches(left: &Path, right: &Path) -> bool {
    left == right
}

fn receipt_from_plan<B: GuardedRenameBatchOps>(
    id: &str,
    plan: &VerifiedRenamePlan,
    batch: &B,
) -> RenameRecoveryReceipt {
    let entries = batch
        .files()
        .iter()
        .map(|file| {
            let stage_path = file.parent_path().join(staging_leaf());
            RenameRecoveryEntry {
                source_ref: file.source_ref().0,
                original_path: file.original_path().to_path_buf(),
                stage_path: stage_path.clone(),
                stage_paths: vec![stage_path],
                final_path: file.target_path().to_path_buf(),
                parent_path: file.parent_path().to_path_buf(),
                parent_identity: file.parent_identity(),
                identity: file.identity(),
                expected_length: file.expected_length(),
                expected_modified: file.expected_modified(),
                content_sha256: file.content_sha256(),
                current_path: Some(file.current_path().to_path_buf()),
                undo_restore_path: None,
                outcome: RenameEntryOutcome::Pending,
            }
        })
        .collect();
    RenameRecoveryReceipt {
        id: id.to_owned(),
        root_path: plan.canonical_root().to_path_buf(),
        root_identity: plan.root_identity(),
        generation: plan.generation().0,
        rules_revision: plan.plan().rules_revision().0,
        status: RenameTransactionStatus::Applying,
        undo_previous_status: None,
        entries,
        pending_move: None,
        message: "The confirmed rename transaction is being applied".to_owned(),
    }
}

fn undo_specs(receipt: &RenameRecoveryReceipt) -> Result<Vec<UndoGuardSpec>, String> {
    validate_receipt(receipt)?;
    if receipt
        .entries
        .iter()
        .all(|entry| entry.current_path.as_ref() == Some(&entry.original_path))
    {
        return Ok(Vec::new());
    }
    let mut specs = Vec::new();
    for entry in &receipt.entries {
        let current = entry
            .current_path
            .as_ref()
            .ok_or_else(|| "a recovery entry has no verified current path".to_owned())?;
        specs.push(UndoGuardSpec {
            source_ref: batch_rename::SnapshotEntryRef(entry.source_ref),
            original_path: entry.original_path.clone(),
            current_path: current.clone(),
            parent_path: entry.parent_path.clone(),
            parent_identity: entry.parent_identity,
            identity: entry.identity,
            expected_length: entry.expected_length,
            expected_modified: entry.expected_modified,
            content_sha256: entry.content_sha256,
        });
    }
    Ok(specs)
}

fn reconcile_receipt(
    receipt: &mut RenameRecoveryReceipt,
    cancellation: &ScanCancellation,
) -> Result<(), String> {
    if matches!(
        receipt.status,
        RenameTransactionStatus::Undone
            | RenameTransactionStatus::Restored
            | RenameTransactionStatus::Cancelled
            | RenameTransactionStatus::Failed
            | RenameTransactionStatus::Refused
    ) {
        return Ok(());
    }
    let interrupted = matches!(
        receipt.status,
        RenameTransactionStatus::Applying | RenameTransactionStatus::Undoing
    );
    let mut unresolved = false;
    for entry in &mut receipt.entries {
        if cancellation.is_cancelled() {
            return Err("rename recovery verification was cancelled".to_owned());
        }
        let candidates = entry_candidate_paths(entry, receipt.pending_move.as_ref());
        let actual = file_operations::inspect_recorded_rename_file(
            &receipt.root_path,
            receipt.root_identity,
            &entry.parent_path,
            entry.parent_identity,
            entry.identity,
            entry.expected_length,
            entry.expected_modified,
            entry.content_sha256,
            &candidates,
            cancellation,
        );
        match actual {
            Ok(paths) if paths.len() == 1 => {
                let Some(current) = paths.into_iter().next() else {
                    return Err("recorded file verification returned no path".to_owned());
                };
                if let Some(pending) = receipt
                    .pending_move
                    .as_ref()
                    .filter(|pending| pending.source_ref == entry.source_ref)
                    && path_matches(&current, &pending.to)
                    && is_staging_path(&pending.to)
                    && ![&entry.original_path, &entry.final_path]
                        .into_iter()
                        .chain(entry.undo_restore_path.iter())
                        .any(|recorded| paths_match_windows(recorded, &pending.to))
                {
                    entry.stage_path = current.clone();
                }
                if let Some(pending) = receipt
                    .pending_move
                    .as_ref()
                    .filter(|pending| pending.source_ref == entry.source_ref)
                    && is_staging_path(&pending.to)
                    && !path_matches(&current, &pending.to)
                    && !path_matches(&entry.stage_path, &pending.to)
                {
                    entry
                        .stage_paths
                        .retain(|path| !paths_match_windows(path, &pending.to));
                }
                if is_staging_path(&current) {
                    entry.stage_path = current.clone();
                    let restore_stage = entry
                        .undo_restore_path
                        .as_ref()
                        .filter(|path| is_staging_path(path))
                        .filter(|path| !paths_match_windows(path, &current))
                        .cloned();
                    entry.stage_paths = vec![current.clone()];
                    if let Some(restore_stage) = restore_stage {
                        entry.stage_paths.push(restore_stage);
                    }
                }
                entry.current_path = Some(current);
            }
            Ok(paths) if paths.is_empty() => {
                entry.current_path = None;
                entry.outcome = RenameEntryOutcome::Retained;
                unresolved = true;
            }
            Ok(_) => {
                entry.current_path = None;
                entry.outcome = RenameEntryOutcome::Retained;
                unresolved = true;
            }
            Err(error) => {
                receipt.message = format!("recorded file could not be verified: {error}");
                entry.outcome = RenameEntryOutcome::Retained;
                unresolved = true;
            }
        }
        if cancellation.is_cancelled() {
            return Err("rename recovery verification was cancelled".to_owned());
        }
    }
    if interrupted || unresolved {
        if !unresolved {
            match receipt.status {
                RenameTransactionStatus::Applying
                    if entries_at(receipt, |entry| Some(&entry.original_path)) =>
                {
                    receipt.status = RenameTransactionStatus::Restored;
                    receipt.message =
                        "Interrupted apply is verified at the original names and bytes".to_owned();
                    for entry in &mut receipt.entries {
                        entry.outcome = RenameEntryOutcome::Restored;
                    }
                }
                RenameTransactionStatus::Applying
                    if entries_at(receipt, |entry| Some(&entry.final_path)) =>
                {
                    receipt.status = RenameTransactionStatus::Applied;
                    receipt.message =
                        "Interrupted apply is verified at every confirmed target".to_owned();
                    for entry in &mut receipt.entries {
                        entry.outcome = RenameEntryOutcome::Applied;
                    }
                }
                RenameTransactionStatus::Undoing
                    if entries_at(receipt, |entry| Some(&entry.original_path)) =>
                {
                    receipt.status = RenameTransactionStatus::Undone;
                    receipt.message =
                        "Interrupted Undo is verified at every original name and byte fingerprint"
                            .to_owned();
                    for entry in &mut receipt.entries {
                        entry.undo_restore_path = None;
                        entry.outcome = RenameEntryOutcome::Restored;
                    }
                    receipt.undo_previous_status = None;
                }
                RenameTransactionStatus::Undoing
                    if entries_at(receipt, |entry| entry.undo_restore_path.as_ref()) =>
                {
                    if let Some(previous) = receipt.undo_previous_status.take() {
                        receipt.status = previous;
                        receipt.message =
                            "Interrupted Undo is verified at its pre-Undo names and bytes"
                                .to_owned();
                        for entry in &mut receipt.entries {
                            entry.undo_restore_path = None;
                        }
                    } else {
                        receipt.status = RenameTransactionStatus::RecoveryRequired;
                    }
                }
                RenameTransactionStatus::RecoveryRequired
                    if receipt.undo_previous_status.is_some()
                        && entries_at(receipt, |entry| Some(&entry.original_path)) =>
                {
                    receipt.status = RenameTransactionStatus::Undone;
                    receipt.message =
                        "Recovery verification confirms every file is at its original name"
                            .to_owned();
                    for entry in &mut receipt.entries {
                        entry.outcome = RenameEntryOutcome::Restored;
                        entry.undo_restore_path = None;
                    }
                    receipt.undo_previous_status = None;
                }
                RenameTransactionStatus::RecoveryRequired
                    if entries_at(receipt, |entry| Some(&entry.original_path)) =>
                {
                    receipt.status = RenameTransactionStatus::Restored;
                    receipt.message =
                        "Recovery verification confirms every file is at its original name"
                            .to_owned();
                    for entry in &mut receipt.entries {
                        entry.outcome = RenameEntryOutcome::Restored;
                        entry.undo_restore_path = None;
                    }
                    receipt.undo_previous_status = None;
                }
                RenameTransactionStatus::RecoveryRequired
                    if entries_at(receipt, |entry| Some(&entry.final_path)) =>
                {
                    receipt.status = RenameTransactionStatus::Applied;
                    receipt.message =
                        "Recovery verification confirms every file is at its confirmed target"
                            .to_owned();
                    for entry in &mut receipt.entries {
                        entry.outcome = RenameEntryOutcome::Applied;
                        entry.undo_restore_path = None;
                    }
                    receipt.undo_previous_status = None;
                }
                _ => {
                    receipt.status = RenameTransactionStatus::RecoveryRequired;
                    if receipt.message.is_empty() {
                        receipt.message = "Interrupted work needs explicit recovery".to_owned();
                    }
                }
            }
            receipt.pending_move = None;
        } else {
            receipt.status = RenameTransactionStatus::RecoveryRequired;
            if interrupted {
                receipt.undo_previous_status = None;
                for entry in &mut receipt.entries {
                    entry.undo_restore_path = None;
                }
            }
            if receipt.message.is_empty() {
                receipt.message = "Interrupted work needs explicit recovery".to_owned();
            }
        }
    }
    let pending_move = receipt.pending_move.clone();
    for entry in &mut receipt.entries {
        normalize_recorded_stage_paths(entry, pending_move.as_ref());
    }
    Ok(())
}

fn normalize_recorded_stage_paths(entry: &mut RenameRecoveryEntry, pending: Option<&PendingMove>) {
    let mut paths = vec![entry.stage_path.clone()];
    for path in entry
        .current_path
        .as_ref()
        .filter(|path| is_staging_path(path))
        .into_iter()
        .chain(
            entry
                .undo_restore_path
                .as_ref()
                .filter(|path| is_staging_path(path)),
        )
        .chain(
            pending
                .filter(|pending| {
                    pending.source_ref == entry.source_ref && is_staging_path(&pending.to)
                })
                .map(|pending| &pending.to),
        )
    {
        if !paths
            .iter()
            .any(|recorded| paths_match_windows(recorded, path))
        {
            paths.push(path.clone());
        }
    }
    entry.stage_paths = paths;
}

fn paths_match_windows(left: &Path, right: &Path) -> bool {
    batch_rename::compare_paths_windows_ordinal(left, right) == std::cmp::Ordering::Equal
}

fn entries_at(
    receipt: &RenameRecoveryReceipt,
    expected: fn(&RenameRecoveryEntry) -> Option<&PathBuf>,
) -> bool {
    receipt.entries.iter().all(|entry| {
        entry
            .current_path
            .as_ref()
            .is_some_and(|current| expected(entry).is_some_and(|path| current == path))
    })
}

fn entry_candidate_paths(
    entry: &RenameRecoveryEntry,
    pending: Option<&PendingMove>,
) -> Vec<PathBuf> {
    let mut paths = vec![
        entry.original_path.clone(),
        entry.stage_path.clone(),
        entry.final_path.clone(),
    ];
    paths.extend(entry.stage_paths.iter().cloned());
    if let Some(current) = &entry.current_path {
        paths.push(current.clone());
    }
    if let Some(undo_restore_path) = &entry.undo_restore_path {
        paths.push(undo_restore_path.clone());
    }
    if let Some(pending) = pending.filter(|pending| pending.source_ref == entry.source_ref) {
        paths.push(pending.from.clone());
        paths.push(pending.to.clone());
    }
    deduplicate_paths(paths)
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for path in paths {
        if !result.iter().any(|existing| existing == &path) {
            result.push(path);
        }
    }
    result
}

fn validate_receipt(receipt: &RenameRecoveryReceipt) -> Result<(), String> {
    if receipt.id.len() != 32 || !receipt.id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("rename receipt has an invalid transaction identity".to_owned());
    }
    batch_rename::ExplicitDirectoryRoot::new(receipt.root_path.clone())
        .map_err(|error| format!("rename receipt root is invalid: {error:?}"))?;
    if receipt.entries.is_empty() || receipt.entries.len() > batch_rename::MAX_SELECTED_ENTRIES {
        return Err("rename receipt has an invalid entry count".to_owned());
    }
    let mut refs = HashSet::with_capacity(receipt.entries.len());
    let mut identities = HashSet::with_capacity(receipt.entries.len());
    let mut total_length = 0_u64;
    for entry in &receipt.entries {
        if !refs.insert(entry.source_ref) {
            return Err("rename receipt contains duplicate source references".to_owned());
        }
        if !identities.insert(entry.identity) {
            return Err(
                "rename receipt contains duplicate native file identities and cannot be recovered unambiguously"
                    .to_owned(),
            );
        }
        if entry.content_sha256.len() != 32 {
            return Err("rename receipt has an invalid SHA-256 fingerprint".to_owned());
        }
        if entry.expected_length > file_operations::MAX_RENAME_HASH_BYTES_PER_FILE {
            return Err(
                "rename receipt exceeds the per-file content verification limit".to_owned(),
            );
        }
        total_length = total_length.saturating_add(entry.expected_length);
        if total_length > file_operations::MAX_RENAME_HASH_BYTES_PER_BATCH {
            return Err("rename receipt exceeds the batch content verification limit".to_owned());
        }
        for path in [
            &entry.original_path,
            &entry.stage_path,
            &entry.final_path,
            &entry.parent_path,
        ]
        .into_iter()
        .chain(entry.current_path.iter())
        .chain(entry.undo_restore_path.iter())
        .chain(entry.stage_paths.iter())
        {
            if !path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, Component::ParentDir))
                || path.components().any(|component| match component {
                    Component::Normal(name) => name.to_string_lossy().contains(':'),
                    _ => false,
                })
                || !batch_rename::path_is_within_windows_ordinal(&receipt.root_path, path)
            {
                return Err(format!(
                    "rename receipt contains an unconfined path: {}",
                    path.display()
                ));
            }
        }
        for path in [&entry.original_path, &entry.stage_path, &entry.final_path]
            .into_iter()
            .chain(entry.current_path.iter())
            .chain(entry.undo_restore_path.iter())
        {
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(batch_rename::is_valid_windows_filename_leaf)
            {
                return Err(format!(
                    "rename receipt contains an invalid Windows filename: {}",
                    path.display()
                ));
            }
        }
        if !same_parent(&entry.original_path, &entry.parent_path)
            || !same_parent(&entry.stage_path, &entry.parent_path)
            || !same_parent(&entry.final_path, &entry.parent_path)
            || entry
                .current_path
                .as_ref()
                .is_some_and(|path| !same_parent(path, &entry.parent_path))
            || entry
                .undo_restore_path
                .as_ref()
                .is_some_and(|path| !same_parent(path, &entry.parent_path))
        {
            return Err("rename receipt entry crossed its recorded parent directory".to_owned());
        }
        if !is_staging_path(&entry.stage_path) {
            return Err("rename receipt contains an invalid staging path".to_owned());
        }
        let pending_stage = receipt
            .pending_move
            .as_ref()
            .filter(|pending| pending.source_ref == entry.source_ref)
            .map(|pending| &pending.to)
            .filter(|path| is_staging_path(path));
        let mut allowed_stage_paths = vec![&entry.stage_path];
        if let Some(current) = entry
            .current_path
            .as_ref()
            .filter(|path| is_staging_path(path))
        {
            allowed_stage_paths.push(current);
        }
        if let Some(restore) = entry
            .undo_restore_path
            .as_ref()
            .filter(|path| is_staging_path(path))
        {
            allowed_stage_paths.push(restore);
        }
        if let Some(pending) = pending_stage {
            allowed_stage_paths.push(pending);
        }
        let stage_path_limit = if pending_stage.is_some() { 3 } else { 2 };
        if entry.stage_paths.is_empty()
            || entry.stage_paths.len() > stage_path_limit
            || allowed_stage_paths.iter().any(|allowed| {
                !entry
                    .stage_paths
                    .iter()
                    .any(|path| paths_match_windows(path, allowed))
            })
            || entry.stage_paths.iter().any(|path| {
                !allowed_stage_paths
                    .iter()
                    .any(|allowed| paths_match_windows(path, allowed))
            })
            || entry.stage_paths.iter().enumerate().any(|(index, path)| {
                entry.stage_paths[..index]
                    .iter()
                    .any(|previous| paths_match_windows(previous, path))
            })
            || entry
                .stage_paths
                .iter()
                .any(|path| !is_staging_path(path) || !same_parent(path, &entry.parent_path))
        {
            return Err("rename receipt contains an invalid recorded staging path set".to_owned());
        }
        if entry
            .current_path
            .as_ref()
            .is_some_and(|current| !is_recorded_entry_path(entry, current))
        {
            return Err(
                "rename receipt current path is outside its recorded state graph".to_owned(),
            );
        }
        if entry.undo_restore_path.as_ref().is_some_and(|restore| {
            ![&entry.original_path, &entry.stage_path, &entry.final_path]
                .into_iter()
                .chain(entry.stage_paths.iter())
                .any(|recorded| paths_match_windows(recorded, restore))
        }) {
            return Err(
                "rename receipt Undo restore path is outside its recorded state graph".to_owned(),
            );
        }
        if let Some(pending) = &receipt.pending_move
            && pending.source_ref == entry.source_ref
        {
            let from_is_current = entry
                .current_path
                .as_ref()
                .is_some_and(|current| paths_match_windows(current, &pending.from));
            let to_is_recorded = is_recorded_entry_path(entry, &pending.to);
            if !same_parent(&pending.from, &entry.parent_path)
                || !same_parent(&pending.to, &entry.parent_path)
                || !from_is_current
                || !to_is_recorded
            {
                return Err("rename receipt contains an unrecognized pending move".to_owned());
            }
        }
        if let Some(pending) = &receipt.pending_move
            && pending.source_ref == entry.source_ref
            && (pending.identity != entry.identity
                || pending.content_sha256 != entry.content_sha256)
        {
            return Err("rename receipt pending move identity does not match its entry".to_owned());
        }
    }
    if receipt
        .pending_move
        .as_ref()
        .is_some_and(|pending| !refs.contains(&pending.source_ref))
    {
        return Err("rename receipt pending move references an unknown entry".to_owned());
    }
    Ok(())
}

fn is_recorded_entry_path(entry: &RenameRecoveryEntry, path: &Path) -> bool {
    [&entry.original_path, &entry.stage_path, &entry.final_path]
        .into_iter()
        .chain(entry.stage_paths.iter())
        .chain(entry.undo_restore_path.iter())
        .any(|recorded| paths_match_windows(recorded, path))
}

fn is_staging_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|leaf| leaf.to_str())
        .is_some_and(|leaf| {
            leaf.starts_with(".mlrn-")
                && leaf.ends_with(".tmp")
                && leaf.len() == 26
                && leaf
                    .get(6..22)
                    .is_some_and(|middle| middle.bytes().all(|byte| byte.is_ascii_hexdigit()))
        })
}

fn same_parent(path: &Path, expected_parent: &Path) -> bool {
    path.parent().is_some_and(|parent| {
        batch_rename::compare_paths_windows_ordinal(parent, expected_parent)
            == std::cmp::Ordering::Equal
    })
}

fn load_journal(path: &Path) -> Result<RecoveryJournal, String> {
    validate_journal_path(path)?;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(RecoveryJournal::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::with_capacity(MAX_JOURNAL_BYTES.saturating_add(1));
    file.take(MAX_JOURNAL_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(format!(
            "rename recovery journal exceeds the {MAX_JOURNAL_BYTES}-byte limit"
        ));
    }
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err("the rename recovery journal is empty".to_owned());
    }
    let journal: RecoveryJournal =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if journal.schema_version != JOURNAL_SCHEMA_VERSION {
        return Err(format!(
            "unsupported rename recovery journal version {}",
            journal.schema_version
        ));
    }
    if journal.receipts.len() > MAX_RETAINED_RENAME_RECEIPTS {
        return Err("the rename recovery journal exceeds its receipt limit".to_owned());
    }
    for receipt in &journal.receipts {
        validate_receipt(receipt)?;
    }
    Ok(journal)
}

fn save_journal(path: &Path, journal: &RecoveryJournal) -> Result<(), String> {
    validate_journal_path(path)?;
    let bytes = serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(format!(
            "rename recovery journal would exceed the {MAX_JOURNAL_BYTES}-byte limit"
        ));
    }
    save_json_atomic(path, journal).map_err(|error| error.to_string())
}

fn validate_journal_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .file_name()
            .is_none_or(|name| name != RECOVERY_JOURNAL_FILE)
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("rename recovery journal path is not the application-owned journal".to_owned());
    }
    Ok(())
}

fn update_receipt(
    journal: &mut RecoveryJournal,
    receipt: &mut RenameRecoveryReceipt,
    path: &Path,
) -> Result<(), String> {
    let Some(stored) = find_receipt_mut(journal, &receipt.id) else {
        return Err("rename receipt was lost from the application journal".to_owned());
    };
    *stored = receipt.clone();
    save_journal(path, journal)
}

fn find_receipt_mut<'a>(
    journal: &'a mut RecoveryJournal,
    id: &str,
) -> Option<&'a mut RenameRecoveryReceipt> {
    journal.receipts.iter_mut().find(|receipt| receipt.id == id)
}

fn staging_leaf() -> OsString {
    OsString::from(format!(".mlrn-{:016x}.tmp", rand::random::<u64>()))
}

fn is_destination_collision(error: &io::Error) -> bool {
    file_operations::rename_error_is_destination_collision(error)
}

fn refused_result(id: String, message: String) -> RenameOperationResult {
    terminal_result(id, RenameTransactionStatus::Refused, message)
}

fn refused_receipt(mut receipt: RenameRecoveryReceipt, message: String) -> RenameOperationResult {
    receipt.message = message;
    RenameOperationResult {
        message: receipt.message.clone(),
        receipt,
    }
}

fn failed_result(receipt: RenameRecoveryReceipt, message: String) -> RenameOperationResult {
    let mut receipt = receipt;
    receipt.status = RenameTransactionStatus::RecoveryRequired;
    receipt.message = message;
    RenameOperationResult {
        message: receipt.message.clone(),
        receipt,
    }
}

fn terminal_result(
    id: String,
    status: RenameTransactionStatus,
    message: String,
) -> RenameOperationResult {
    RenameOperationResult {
        receipt: RenameRecoveryReceipt {
            id,
            root_path: PathBuf::new(),
            root_identity: NativeFileIdentity::default(),
            generation: 0,
            rules_revision: 0,
            status,
            undo_previous_status: None,
            entries: Vec::new(),
            pending_move: None,
            message: message.clone(),
        },
        message,
    }
}

#[cfg(test)]
mod m8d_tests {
    use super::*;
    use crate::batch_rename::{
        CaseConversion, ExplicitDirectoryRoot, ExtensionPolicy, RenameRule, RenameRuleSet,
        RulesRevision, SnapshotEntryKind, SnapshotEntryRef,
    };
    use crate::file_operations::{RenameScanGeneration, RenameScanRequest, RenameScanStatus};
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    #[derive(Clone)]
    struct RowProof {
        entry_ref: SnapshotEntryRef,
        path: PathBuf,
        identity: NativeFileIdentity,
        parent_identity: NativeFileIdentity,
        length: u64,
        modified: SystemTime,
    }

    fn workspace() -> TempDir {
        let path = std::env::current_dir()
            .expect("workspace current directory")
            .join("target")
            .join("productivity_expansion");
        std::fs::create_dir_all(&path).expect("create disposable M8-D test workspace");
        tempfile::Builder::new()
            .prefix("m8d-transaction-")
            .tempdir_in(path)
            .expect("create M8-D fixture directory")
    }

    fn create_root(workspace: &TempDir, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let root = workspace.path().join(name);
        std::fs::create_dir_all(&root).expect("create fixture root");
        for (name, bytes) in files {
            std::fs::write(root.join(name), bytes).expect("create fixture file");
        }
        root
    }

    fn verified_plan(
        root: &Path,
        selected_names: &[&str],
        rules: RenameRuleSet,
    ) -> (VerifiedRenamePlan, Vec<RowProof>) {
        let generation = RenameScanGeneration(41);
        let result = file_operations::scan_rename_root(
            RenameScanRequest::new(
                ExplicitDirectoryRoot::new(root.to_path_buf()).expect("absolute fixture root"),
                false,
                generation,
            ),
            &ScanCancellation::new(),
        );
        assert_eq!(result.status(), RenameScanStatus::Complete);
        let row_proofs = result
            .rows()
            .iter()
            .filter(|row| row.kind() == SnapshotEntryKind::File)
            .map(|row| RowProof {
                entry_ref: row.entry_ref(),
                path: row.path().to_path_buf(),
                identity: row.identity(),
                parent_identity: row.parent_identity(),
                length: row.length().expect("fixture file length"),
                modified: row.modified().expect("fixture file modification time"),
            })
            .collect::<Vec<_>>();
        let mut snapshot = result.accept(generation).expect("accept complete scan");
        for row in &row_proofs {
            let name = row.path.file_name().and_then(|name| name.to_str()).unwrap();
            snapshot = snapshot
                .with_selection(row.entry_ref, selected_names.contains(&name))
                .expect("set fixture selection");
        }
        let preview = snapshot.preview(&rules.with_revision(RulesRevision(7)));
        assert_eq!(
            preview.preview().readiness(),
            batch_rename::PreviewReadiness::Ready
        );
        let plan = preview.verified_plan().expect("verified fixture plan");
        assert!(!plan.plan().actions().is_empty());
        (plan, row_proofs)
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct FakeObject {
        identity: NativeFileIdentity,
        content_sha256: [u8; 32],
    }

    #[derive(Default)]
    struct FakeFilesystem {
        objects: HashMap<PathBuf, FakeObject>,
        journal_path: Option<PathBuf>,
        fail_destination: Option<PathBuf>,
        insert_on_failure: Option<(PathBuf, FakeObject)>,
        failure_used: bool,
        collide_with_first_stage: bool,
        stage_collision_used: bool,
        stage_collision_path: Option<PathBuf>,
        fail_after_stage_move: bool,
        post_move_failure_used: bool,
        renamed_objects: usize,
        mutation_before_journal: bool,
    }

    struct FakeGuardedFile {
        source_ref: SnapshotEntryRef,
        original_path: PathBuf,
        target_path: PathBuf,
        current_path: PathBuf,
        parent_path: PathBuf,
        parent_identity: NativeFileIdentity,
        identity: NativeFileIdentity,
        expected_length: u64,
        expected_modified: SystemTime,
        content_sha256: [u8; 32],
        filesystem: Arc<Mutex<FakeFilesystem>>,
    }

    struct FakeGuardedBatch {
        files: Vec<FakeGuardedFile>,
    }

    impl GuardedRenameFileOps for FakeGuardedFile {
        fn source_ref(&self) -> SnapshotEntryRef {
            self.source_ref
        }

        fn original_path(&self) -> &Path {
            &self.original_path
        }

        fn target_path(&self) -> &Path {
            &self.target_path
        }

        fn current_path(&self) -> &Path {
            &self.current_path
        }

        fn parent_path(&self) -> &Path {
            &self.parent_path
        }

        fn parent_identity(&self) -> NativeFileIdentity {
            self.parent_identity
        }

        fn identity(&self) -> NativeFileIdentity {
            self.identity
        }

        fn expected_length(&self) -> u64 {
            self.expected_length
        }

        fn expected_modified(&self) -> SystemTime {
            self.expected_modified
        }

        fn content_sha256(&self) -> [u8; 32] {
            self.content_sha256
        }

        fn rename_to_leaf(
            &mut self,
            leaf: &std::ffi::OsStr,
            verification_mode: ContentVerificationMode<'_>,
        ) -> io::Result<PathBuf> {
            self.verify_at(&self.current_path, verification_mode)?;
            let destination = self.parent_path.join(leaf);
            let mut filesystem = self.filesystem.lock().unwrap();
            if filesystem
                .journal_path
                .as_ref()
                .is_none_or(|journal| !journal.exists())
            {
                filesystem.mutation_before_journal = true;
                return Err(io::Error::other("fake mutation preceded journal creation"));
            }
            if filesystem.collide_with_first_stage
                && !filesystem.stage_collision_used
                && leaf.to_string_lossy().starts_with(".mlrn-")
            {
                filesystem.stage_collision_used = true;
                filesystem.stage_collision_path = Some(destination.clone());
                filesystem.objects.insert(
                    destination,
                    FakeObject {
                        identity: NativeFileIdentity::default(),
                        content_sha256: Sha256::digest(b"unselected stage occupant").into(),
                    },
                );
                return Err(io::Error::from_raw_os_error(183));
            }
            if filesystem.fail_destination.as_ref() == Some(&destination)
                && !filesystem.failure_used
            {
                filesystem.failure_used = true;
                if let Some((path, object)) = filesystem.insert_on_failure.take() {
                    filesystem.objects.insert(path, object);
                }
                return Err(io::Error::other("injected publication failure"));
            }
            if filesystem.objects.contains_key(&destination) {
                return Err(io::Error::from_raw_os_error(183));
            }
            let Some(object) = filesystem.objects.remove(&self.current_path) else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "fake source no longer occupies its guarded path",
                ));
            };
            filesystem.objects.insert(destination.clone(), object);
            filesystem.renamed_objects += 1;
            let fail_after_stage_move = filesystem.fail_after_stage_move
                && !filesystem.post_move_failure_used
                && leaf.to_string_lossy().starts_with(".mlrn-");
            if fail_after_stage_move {
                filesystem.post_move_failure_used = true;
            }
            self.current_path = destination.clone();
            if fail_after_stage_move {
                Err(io::Error::other(
                    "injected post-move stage verification error",
                ))
            } else {
                Ok(destination)
            }
        }

        fn verify_at(
            &self,
            expected_path: &Path,
            verification_mode: ContentVerificationMode<'_>,
        ) -> io::Result<()> {
            if verification_mode.is_cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "fake content verification was cancelled",
                ));
            }
            if self.current_path != expected_path {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fake guarded file path changed",
                ));
            }
            let filesystem = self.filesystem.lock().unwrap();
            match filesystem.objects.get(expected_path) {
                Some(object)
                    if object.identity == self.identity
                        && object.content_sha256 == self.content_sha256 =>
                {
                    Ok(())
                }
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fake guarded identity or content fingerprint changed",
                )),
            }
        }
    }

    impl GuardedRenameBatchOps for FakeGuardedBatch {
        type File = FakeGuardedFile;

        fn files(&self) -> &[Self::File] {
            &self.files
        }

        fn files_mut(&mut self) -> &mut [Self::File] {
            &mut self.files
        }
    }

    fn fake_batch(
        plan: &VerifiedRenamePlan,
        rows: &[RowProof],
        journal_path: &Path,
    ) -> (FakeGuardedBatch, Arc<Mutex<FakeFilesystem>>) {
        let rows_by_ref = rows
            .iter()
            .map(|row| (row.entry_ref, row))
            .collect::<HashMap<_, _>>();
        let filesystem = Arc::new(Mutex::new(FakeFilesystem {
            journal_path: Some(journal_path.to_path_buf()),
            ..FakeFilesystem::default()
        }));
        {
            let mut state = filesystem.lock().unwrap();
            for row in rows {
                let bytes = std::fs::read(&row.path).expect("read fake fixture bytes");
                state.objects.insert(
                    row.path.clone(),
                    FakeObject {
                        identity: row.identity,
                        content_sha256: Sha256::digest(bytes).into(),
                    },
                );
            }
        }
        let files = plan
            .plan()
            .actions()
            .iter()
            .map(|action| {
                let row = rows_by_ref[&action.source_ref()];
                let object = filesystem.lock().unwrap().objects[&row.path];
                FakeGuardedFile {
                    source_ref: action.source_ref(),
                    original_path: action.source_path().to_path_buf(),
                    target_path: action.target_path().to_path_buf(),
                    current_path: action.source_path().to_path_buf(),
                    parent_path: action.source_path().parent().unwrap().to_path_buf(),
                    parent_identity: row.parent_identity,
                    identity: object.identity,
                    expected_length: row.length,
                    expected_modified: row.modified,
                    content_sha256: object.content_sha256,
                    filesystem: Arc::clone(&filesystem),
                }
            })
            .collect();
        (FakeGuardedBatch { files }, filesystem)
    }

    fn fake_apply(
        plan: &VerifiedRenamePlan,
        mut batch: FakeGuardedBatch,
        journal_path: &Path,
        transaction_id: &str,
    ) -> RenameOperationResult {
        let mut journal = RecoveryJournal::default();
        let mut receipt = receipt_from_plan(transaction_id, plan, &batch);
        journal.receipts.push(receipt.clone());
        if let Err(error) = save_journal(journal_path, &journal) {
            return failed_result(receipt, error);
        }
        let mut report = |_| {};
        let cancellation = ScanCancellation::new();
        for index in 0..batch.files().len() {
            if let Err(error) = stage_entry(
                &mut journal,
                &mut receipt,
                &mut batch,
                index,
                journal_path,
                transaction_id,
                ContentVerificationMode::Cooperative(&cancellation),
            ) {
                return match error {
                    StepFailure::Mutation(message) => rollback_apply(
                        &mut journal,
                        &mut receipt,
                        &mut batch,
                        journal_path,
                        transaction_id,
                        &message,
                        false,
                        &mut report,
                    ),
                    error => stop_for_recovery(
                        &mut journal,
                        &mut receipt,
                        transaction_id,
                        error.to_string(),
                    ),
                };
            }
        }
        for index in 0..batch.files().len() {
            if let Err(error) = publish_entry(
                &mut journal,
                &mut receipt,
                &mut batch,
                index,
                journal_path,
                transaction_id,
                ContentVerificationMode::Cooperative(&cancellation),
            ) {
                return match error {
                    StepFailure::Mutation(message) => rollback_apply(
                        &mut journal,
                        &mut receipt,
                        &mut batch,
                        journal_path,
                        transaction_id,
                        &message,
                        false,
                        &mut report,
                    ),
                    error => stop_for_recovery(
                        &mut journal,
                        &mut receipt,
                        transaction_id,
                        error.to_string(),
                    ),
                };
            }
        }
        receipt.status = RenameTransactionStatus::Applied;
        receipt.message = "fake transaction applied".to_owned();
        receipt.pending_move = None;
        journal.receipts[0] = receipt.clone();
        if let Err(error) = save_journal(journal_path, &journal) {
            return failed_result(receipt, error);
        }
        RenameOperationResult {
            message: receipt.message.clone(),
            receipt,
        }
    }

    fn fake_preflight_undo(
        receipt: &RenameRecoveryReceipt,
        filesystem: &FakeFilesystem,
    ) -> Result<Vec<UndoGuardSpec>, String> {
        let specs = undo_specs(receipt)?;
        for spec in &specs {
            let Some(object) = filesystem.objects.get(&spec.current_path) else {
                return Err("fake Undo source is missing".to_owned());
            };
            if object.identity != spec.identity || object.content_sha256 != spec.content_sha256 {
                return Err("fake Undo source identity or bytes changed".to_owned());
            }
        }
        for spec in &specs {
            if let Some(occupant) = filesystem.objects.get(&spec.original_path) {
                let selected_source = specs
                    .iter()
                    .find(|candidate| candidate.current_path == spec.original_path);
                if selected_source.is_none_or(|source| source.identity != occupant.identity) {
                    return Err(format!(
                        "fake Undo destination is occupied: {}",
                        spec.original_path.display()
                    ));
                }
            }
        }
        Ok(specs)
    }

    fn prefix_rules(prefix: &str) -> RenameRuleSet {
        RenameRuleSet::new(
            ExtensionPolicy::Preserve,
            vec![RenameRule::Prefix(prefix.to_owned())],
        )
    }

    #[cfg(windows)]
    #[test]
    fn m8d_fake_journal_unknown_schemas_and_fields_preserve_original_bytes() {
        let fixture = workspace();
        let root = create_root(&fixture, "strict-journal", &[("a.txt", b"payload")]);
        let (plan, rows) = verified_plan(&root, &["a.txt"], prefix_rules("new-"));
        let journal_path = fixture
            .path()
            .join("strict-journal-store")
            .join(RECOVERY_JOURNAL_FILE);
        std::fs::create_dir_all(journal_path.parent().unwrap()).unwrap();
        let (batch, _) = fake_batch(&plan, &rows, &journal_path);
        let mut journal = RecoveryJournal::default();
        journal.receipts.push(receipt_from_plan(
            "00000000000000000000000000000091",
            &plan,
            &batch,
        ));
        save_journal(&journal_path, &journal).unwrap();
        let base = serde_json::to_value(&journal).unwrap();

        let mut invalid_journals = Vec::new();
        let mut unknown_root = base.clone();
        unknown_root["future_root_field"] = serde_json::Value::Bool(true);
        invalid_journals.push(unknown_root);

        let mut unknown_receipt = base.clone();
        unknown_receipt["receipts"][0]["future_receipt_field"] = serde_json::Value::Bool(true);
        invalid_journals.push(unknown_receipt);

        let mut unknown_entry = base.clone();
        unknown_entry["receipts"][0]["entries"][0]["future_entry_field"] =
            serde_json::Value::Bool(true);
        invalid_journals.push(unknown_entry);

        let mut unknown_identity = base.clone();
        unknown_identity["receipts"][0]["entries"][0]["identity"]["future_identity_field"] =
            serde_json::Value::Bool(true);
        invalid_journals.push(unknown_identity);

        let mut unknown_pending = base.clone();
        let entry = &base["receipts"][0]["entries"][0];
        unknown_pending["receipts"][0]["pending_move"] = serde_json::json!({
            "source_ref": entry["source_ref"],
            "from": entry["original_path"],
            "to": entry["stage_path"],
            "identity": entry["identity"],
            "content_sha256": entry["content_sha256"],
            "future_pending_field": true
        });
        invalid_journals.push(unknown_pending);

        let mut unsupported_version = base;
        unsupported_version["schema_version"] = serde_json::Value::from(999_u64);
        invalid_journals.push(unsupported_version);

        for invalid in invalid_journals {
            let original = serde_json::to_vec_pretty(&invalid).unwrap();
            std::fs::write(&journal_path, &original).unwrap();
            assert!(load_and_reconcile_journal(&journal_path, &ScanCancellation::new()).is_err());
            assert_eq!(std::fs::read(&journal_path).unwrap(), original);
        }
    }

    #[cfg(windows)]
    #[test]
    fn m8d_fake_journal_allows_only_the_bounded_pending_undo_stage_graph() {
        let fixture = workspace();
        let root = create_root(&fixture, "pending-undo-stage", &[("a.txt", b"payload")]);
        let (plan, rows) = verified_plan(&root, &["a.txt"], prefix_rules("new-"));
        let journal_path = fixture
            .path()
            .join("pending-undo-stage-store")
            .join(RECOVERY_JOURNAL_FILE);
        std::fs::create_dir_all(journal_path.parent().unwrap()).unwrap();
        let (batch, _) = fake_batch(&plan, &rows, &journal_path);
        let mut receipt = receipt_from_plan("00000000000000000000000000000092", &plan, &batch);
        let entry = &mut receipt.entries[0];
        let parent = entry.parent_path.clone();
        let current_stage = parent.join(".mlrn-1111111111111111.tmp");
        let undo_restore_stage = parent.join(".mlrn-2222222222222222.tmp");
        let pending_stage = parent.join(".mlrn-3333333333333333.tmp");
        entry.stage_path = current_stage.clone();
        entry.stage_paths = vec![
            current_stage.clone(),
            undo_restore_stage.clone(),
            pending_stage.clone(),
        ];
        entry.current_path = Some(current_stage.clone());
        entry.undo_restore_path = Some(undo_restore_stage.clone());
        receipt.status = RenameTransactionStatus::Undoing;
        receipt.undo_previous_status = Some(RenameTransactionStatus::Applied);
        receipt.pending_move = Some(PendingMove {
            source_ref: entry.source_ref,
            from: current_stage,
            to: pending_stage,
            identity: entry.identity,
            content_sha256: entry.content_sha256,
        });
        assert!(validate_receipt(&receipt).is_ok());

        receipt.entries[0]
            .stage_paths
            .push(parent.join(".mlrn-4444444444444444.tmp"));
        assert!(validate_receipt(&receipt).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn m8d_fake_post_move_stage_error_keeps_owned_payload_reopenable() {
        let fixture = workspace();
        let root = create_root(&fixture, "post-move-stage", &[("a.txt", b"owned payload")]);
        let (plan, rows) = verified_plan(&root, &["a.txt"], prefix_rules("new-"));
        let journal_path = fixture
            .path()
            .join("post-move-stage-store")
            .join(RECOVERY_JOURNAL_FILE);
        std::fs::create_dir_all(journal_path.parent().unwrap()).unwrap();
        let (batch, filesystem) = fake_batch(&plan, &rows, &journal_path);
        filesystem.lock().unwrap().fail_after_stage_move = true;

        let result = fake_apply(
            &plan,
            batch,
            &journal_path,
            "00000000000000000000000000000093",
        );
        assert_eq!(result.receipt.status(), RenameTransactionStatus::Restored);
        let loaded = load_journal(&journal_path).expect("the compensated journal reopens");
        let recovered = &loaded.receipts[0];
        assert_eq!(recovered.status, RenameTransactionStatus::Restored);
        assert_eq!(
            recovered.entries[0].current_path.as_deref(),
            Some(root.join("a.txt").as_path())
        );
        let state = filesystem.lock().unwrap();
        let payload = state.objects.get(&root.join("a.txt")).unwrap();
        assert_eq!(payload.identity, rows[0].identity);
        assert_eq!(
            payload.content_sha256,
            <[u8; 32]>::from(Sha256::digest(b"owned payload"))
        );
        assert_eq!(state.objects.len(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn m8d_fake_journal_and_no_replace_staging_preserve_unselected_occupants() {
        let fixture = workspace();
        let root = create_root(
            &fixture,
            "fake-no-replace",
            &[
                ("a.txt", b"selected payload"),
                ("keep.txt", b"user occupant"),
            ],
        );
        let (plan, rows) = verified_plan(&root, &["a.txt"], prefix_rules("new-"));
        let journal_path = fixture.path().join(RECOVERY_JOURNAL_FILE);
        let (batch, filesystem) = fake_batch(&plan, &rows, &journal_path);
        filesystem.lock().unwrap().collide_with_first_stage = true;
        let unselected = rows
            .iter()
            .find(|row| row.path.file_name().unwrap() == "keep.txt")
            .unwrap();
        let unselected_bytes = std::fs::read(&unselected.path).unwrap();
        let unselected_object = filesystem.lock().unwrap().objects[&unselected.path];

        let result = fake_apply(
            &plan,
            batch,
            &journal_path,
            "00000000000000000000000000000011",
        );
        assert_eq!(result.receipt.status(), RenameTransactionStatus::Applied);
        let state = filesystem.lock().unwrap();
        let renamed = state.objects[&root.join("new-a.txt")];
        assert_eq!(renamed.identity, rows[0].identity);
        assert_eq!(
            renamed.content_sha256,
            <[u8; 32]>::from(Sha256::digest(b"selected payload"))
        );
        assert!(!state.objects.contains_key(&root.join("a.txt")));
        assert_eq!(state.objects[&unselected.path], unselected_object);
        assert_eq!(std::fs::read(&unselected.path).unwrap(), unselected_bytes);
        assert!(!state.mutation_before_journal);
        let collision = state.stage_collision_path.as_ref().unwrap();
        assert!(state.objects.contains_key(collision));
        assert_eq!(
            state.objects[collision].identity,
            NativeFileIdentity::default()
        );
        assert!(
            journal_path.exists(),
            "the app-owned receipt precedes moves"
        );
    }

    #[cfg(windows)]
    #[test]
    fn m8d_fake_partial_compensation_keeps_payload_and_refuses_occupied_undo() {
        let fixture = workspace();
        let root = create_root(
            &fixture,
            "fake-partial",
            &[("a.txt", b"alpha"), ("b.txt", b"beta")],
        );
        let (plan, rows) = verified_plan(&root, &["a.txt", "b.txt"], prefix_rules("new-"));
        let journal_path = fixture.path().join(RECOVERY_JOURNAL_FILE);
        let (batch, filesystem) = fake_batch(&plan, &rows, &journal_path);
        let blocker_path = root.join("a.txt");
        let blocker = FakeObject {
            identity: NativeFileIdentity::default(),
            content_sha256: Sha256::digest(b"newer user file").into(),
        };
        {
            let mut state = filesystem.lock().unwrap();
            state.fail_destination = Some(root.join("new-b.txt"));
            state.insert_on_failure = Some((blocker_path.clone(), blocker));
        }

        let result = fake_apply(
            &plan,
            batch,
            &journal_path,
            "00000000000000000000000000000022",
        );
        assert_eq!(
            result.receipt.status(),
            RenameTransactionStatus::RecoveryRequired
        );
        let state = filesystem.lock().unwrap();
        assert_eq!(state.objects[&blocker_path], blocker);
        let mut retained = 0;
        for entry in result.receipt.entries() {
            let current = entry
                .current_path()
                .expect("receipt has known payload path");
            let object = state
                .objects
                .get(current)
                .expect("owned payload is retained");
            assert_eq!(object.identity, entry.identity);
            assert_eq!(object.content_sha256, entry.content_sha256);
            retained += usize::from(current != entry.original_path());
        }
        assert_eq!(
            retained, 1,
            "one applied payload remains staged for recovery"
        );
        let rename_count = state.renamed_objects;
        let undo = fake_preflight_undo(&result.receipt, &state);
        assert!(undo.is_err_and(|error| error.contains("occupied")));
        assert_eq!(
            state.renamed_objects, rename_count,
            "refused Undo moves nothing"
        );
        assert_eq!(state.objects[&blocker_path], blocker);
    }

    #[cfg(windows)]
    #[test]
    fn m8d_windows_fixture_journal_failure_refuses_before_any_rename() {
        let fixture = workspace();
        let root = create_root(
            &fixture,
            "journal-failure",
            &[("alpha.txt", b"alpha bytes")],
        );
        let (plan, rows) = verified_plan(&root, &["alpha.txt"], prefix_rules("renamed-"));
        let original = &rows[0];
        let journal_path = fixture.path().join("journal-is-a-directory");
        std::fs::create_dir(&journal_path).unwrap();

        let result = execute_apply(plan, &journal_path, &ScanCancellation::new(), |_| {});
        assert_eq!(result.receipt.status(), RenameTransactionStatus::Refused);
        assert!(!result.message.is_empty());
        assert_eq!(std::fs::read(&original.path).unwrap(), b"alpha bytes");
        assert_eq!(identity_at(&root, "alpha.txt"), original.identity);
        assert_eq!(directory_names(&root), vec!["alpha.txt"]);
        assert!(std::fs::read_dir(&journal_path).unwrap().next().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn m8d_windows_fixture_ordinary_protected_extension_and_undo_preserve_identity() {
        let fixture = workspace();
        let root = create_root(
            &fixture,
            "ordinary",
            &[("alpha.txt", b"alpha bytes"), ("keep.log", b"other bytes")],
        );
        let (plan, rows) = verified_plan(&root, &["alpha.txt"], prefix_rules("renamed-"));
        let alpha = rows
            .iter()
            .find(|row| row.path.ends_with("alpha.txt"))
            .unwrap();
        let keep = rows
            .iter()
            .find(|row| row.path.ends_with("keep.log"))
            .unwrap();
        let journal_path = fixture.path().join(RECOVERY_JOURNAL_FILE);

        let applied = execute_apply(plan, &journal_path, &ScanCancellation::new(), |_| {});
        assert_eq!(
            applied.receipt.status(),
            RenameTransactionStatus::Applied,
            "{}",
            applied.message
        );
        let renamed = root.join("renamed-alpha.txt");
        assert_eq!(std::fs::read(&renamed).unwrap(), b"alpha bytes");
        assert_eq!(identity_at(&root, "renamed-alpha.txt"), alpha.identity);
        assert_eq!(identity_at(&root, "keep.log"), keep.identity);
        assert!(!applied.receipt.entries()[0].stage_path.exists());

        let undone = execute_undo(
            applied.receipt.id(),
            &journal_path,
            &ScanCancellation::new(),
            |_| {},
        );
        assert_eq!(undone.receipt.status(), RenameTransactionStatus::Undone);
        assert_eq!(
            std::fs::read(root.join("alpha.txt")).unwrap(),
            b"alpha bytes"
        );
        assert_eq!(identity_at(&root, "alpha.txt"), alpha.identity);
        assert_eq!(identity_at(&root, "keep.log"), keep.identity);
        assert!(!renamed.exists());
        assert!(!undone.receipt.entries()[0].stage_path.exists());
    }

    #[cfg(windows)]
    #[test]
    fn m8d_windows_fixture_case_only_and_two_file_swap_round_trip() {
        let fixture = workspace();
        let case_root = create_root(&fixture, "case-only", &[("alpha.txt", b"case bytes")]);
        let (case_plan, case_rows) = verified_plan(
            &case_root,
            &["alpha.txt"],
            RenameRuleSet::new(
                ExtensionPolicy::Preserve,
                vec![RenameRule::Case(CaseConversion::Uppercase)],
            ),
        );
        let case_identity = case_rows[0].identity;
        let case_journal = fixture
            .path()
            .join("case-journal")
            .join(RECOVERY_JOURNAL_FILE);
        std::fs::create_dir_all(case_journal.parent().unwrap()).unwrap();
        let case_applied =
            execute_apply(case_plan, &case_journal, &ScanCancellation::new(), |_| {});
        assert_eq!(
            case_applied.receipt.status(),
            RenameTransactionStatus::Applied,
            "{}",
            case_applied.message
        );
        assert_eq!(directory_names(&case_root), vec!["ALPHA.txt"]);
        assert_eq!(identity_at(&case_root, "ALPHA.txt"), case_identity);
        let case_undone = execute_undo(
            case_applied.receipt.id(),
            &case_journal,
            &ScanCancellation::new(),
            |_| {},
        );
        assert_eq!(
            case_undone.receipt.status(),
            RenameTransactionStatus::Undone
        );
        assert_eq!(directory_names(&case_root), vec!["alpha.txt"]);
        assert_eq!(identity_at(&case_root, "alpha.txt"), case_identity);

        let swap_root = create_root(
            &fixture,
            "two-file-swap",
            &[("a.txt", b"payload a"), ("b.txt", b"payload b")],
        );
        let swap_rules = RenameRuleSet::new(
            ExtensionPolicy::Preserve,
            vec![
                RenameRule::RegexReplace {
                    pattern: "^a$".to_owned(),
                    replace_with: "temporary-a".to_owned(),
                },
                RenameRule::RegexReplace {
                    pattern: "^b$".to_owned(),
                    replace_with: "a".to_owned(),
                },
                RenameRule::RegexReplace {
                    pattern: "^temporary-a$".to_owned(),
                    replace_with: "b".to_owned(),
                },
            ],
        );
        let (swap_plan, swap_rows) = verified_plan(&swap_root, &["a.txt", "b.txt"], swap_rules);
        let a_id = swap_rows
            .iter()
            .find(|row| row.path.ends_with("a.txt"))
            .unwrap()
            .identity;
        let b_id = swap_rows
            .iter()
            .find(|row| row.path.ends_with("b.txt"))
            .unwrap()
            .identity;
        let swap_journal = fixture
            .path()
            .join("swap-journal")
            .join(RECOVERY_JOURNAL_FILE);
        std::fs::create_dir_all(swap_journal.parent().unwrap()).unwrap();
        let applied = execute_apply(swap_plan, &swap_journal, &ScanCancellation::new(), |_| {});
        assert_eq!(applied.receipt.status(), RenameTransactionStatus::Applied);
        assert_eq!(
            std::fs::read(swap_root.join("a.txt")).unwrap(),
            b"payload b"
        );
        assert_eq!(
            std::fs::read(swap_root.join("b.txt")).unwrap(),
            b"payload a"
        );
        assert_eq!(identity_at(&swap_root, "a.txt"), b_id);
        assert_eq!(identity_at(&swap_root, "b.txt"), a_id);
        let undone = execute_undo(
            applied.receipt.id(),
            &swap_journal,
            &ScanCancellation::new(),
            |_| {},
        );
        assert_eq!(undone.receipt.status(), RenameTransactionStatus::Undone);
        assert_eq!(
            std::fs::read(swap_root.join("a.txt")).unwrap(),
            b"payload a"
        );
        assert_eq!(
            std::fs::read(swap_root.join("b.txt")).unwrap(),
            b"payload b"
        );
        assert_eq!(identity_at(&swap_root, "a.txt"), a_id);
        assert_eq!(identity_at(&swap_root, "b.txt"), b_id);
        assert!(
            undone
                .receipt
                .entries()
                .iter()
                .all(|entry| !entry.stage_path.exists())
        );
    }

    #[cfg(windows)]
    #[test]
    fn m8d_windows_fixture_three_file_cycle_no_replace_and_occupied_undo_retain_receipt() {
        let fixture = workspace();
        let root = create_root(
            &fixture,
            "three-file-cycle",
            &[
                ("a.txt", b"payload a"),
                ("b.txt", b"payload b"),
                ("c.txt", b"payload c"),
            ],
        );
        let rules = RenameRuleSet::new(
            ExtensionPolicy::Preserve,
            vec![
                RenameRule::RegexReplace {
                    pattern: "^a$".to_owned(),
                    replace_with: "temporary-a".to_owned(),
                },
                RenameRule::RegexReplace {
                    pattern: "^b$".to_owned(),
                    replace_with: "temporary-b".to_owned(),
                },
                RenameRule::RegexReplace {
                    pattern: "^c$".to_owned(),
                    replace_with: "a".to_owned(),
                },
                RenameRule::RegexReplace {
                    pattern: "^temporary-a$".to_owned(),
                    replace_with: "b".to_owned(),
                },
                RenameRule::RegexReplace {
                    pattern: "^temporary-b$".to_owned(),
                    replace_with: "c".to_owned(),
                },
            ],
        );
        let (plan, rows) = verified_plan(&root, &["a.txt", "b.txt", "c.txt"], rules);
        let identities = rows
            .iter()
            .map(|row| (row.path.file_name().unwrap().to_owned(), row.identity))
            .collect::<HashMap<_, _>>();
        let journal_path = fixture.path().join(RECOVERY_JOURNAL_FILE);
        let blocker_bytes = b"new user file arrived after preflight";
        let mut injected = false;
        let result = execute_apply(plan, &journal_path, &ScanCancellation::new(), |progress| {
            if !injected
                && progress.phase == RenameTransactionPhase::Publishing
                && progress.completed == 0
            {
                std::fs::write(root.join("b.txt"), blocker_bytes)
                    .expect("inject a destination after all files were staged");
                injected = true;
            }
        });
        assert!(
            injected,
            "the test reached publication after staging the full cycle"
        );
        assert_eq!(
            result.receipt.status(),
            RenameTransactionStatus::RecoveryRequired
        );
        assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), blocker_bytes);
        assert_ne!(
            identity_at(&root, "b.txt"),
            identities[std::ffi::OsStr::new("b.txt")]
        );
        assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"payload a");
        assert_eq!(
            identity_at(&root, "a.txt"),
            identities[std::ffi::OsStr::new("a.txt")]
        );
        assert_eq!(std::fs::read(root.join("c.txt")).unwrap(), b"payload c");
        assert_eq!(
            identity_at(&root, "c.txt"),
            identities[std::ffi::OsStr::new("c.txt")]
        );
        let retained = result
            .receipt
            .entries()
            .iter()
            .find(|entry| entry.identity == identities[std::ffi::OsStr::new("b.txt")])
            .unwrap();
        let retained_path = retained.current_path().expect("recovery path is recorded");
        assert_eq!(std::fs::read(retained_path).unwrap(), b"payload b");
        assert_eq!(
            identity_at(
                retained_path.parent().unwrap(),
                retained_path.file_name().unwrap().to_str().unwrap()
            ),
            retained.identity
        );
        assert!(retained_path.exists());

        let undo = execute_undo(
            result.receipt.id(),
            &journal_path,
            &ScanCancellation::new(),
            |_| {},
        );
        assert_eq!(
            undo.receipt.status(),
            RenameTransactionStatus::RecoveryRequired
        );
        assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), blocker_bytes);
        assert_eq!(std::fs::read(retained_path).unwrap(), b"payload b");
        assert_eq!(
            identity_at(
                retained_path.parent().unwrap(),
                retained_path.file_name().unwrap().to_str().unwrap()
            ),
            retained.identity
        );
    }

    #[cfg(windows)]
    #[test]
    fn m8d_windows_fixture_undo_refuses_same_identity_with_changed_bytes() {
        let fixture = workspace();
        let root = create_root(&fixture, "changed-undo", &[("a.txt", b"original bytes")]);
        let (plan, rows) = verified_plan(&root, &["a.txt"], prefix_rules("new-"));
        let original_identity = rows[0].identity;
        let journal_path = fixture.path().join(RECOVERY_JOURNAL_FILE);
        let applied = execute_apply(plan, &journal_path, &ScanCancellation::new(), |_| {});
        assert_eq!(
            applied.receipt.status(),
            RenameTransactionStatus::Applied,
            "{}",
            applied.message
        );
        let changed = root.join("new-a.txt");
        std::fs::write(&changed, b"newer in-place bytes").unwrap();
        assert_eq!(identity_at(&root, "new-a.txt"), original_identity);

        let undo = execute_undo(
            applied.receipt.id(),
            &journal_path,
            &ScanCancellation::new(),
            |_| {},
        );
        assert_eq!(
            undo.receipt.status(),
            RenameTransactionStatus::RecoveryRequired
        );
        assert_eq!(std::fs::read(&changed).unwrap(), b"newer in-place bytes");
        assert!(!root.join("a.txt").exists());
    }

    #[cfg(windows)]
    #[test]
    fn m8d_windows_fixture_reopens_pre_move_stage_journal_without_path_guessing() {
        let fixture = workspace();
        let root = create_root(&fixture, "reopen-stage", &[("a.txt", b"staged payload")]);
        let (plan, rows) = verified_plan(&root, &["a.txt"], prefix_rules("new-"));
        let journal_path = fixture.path().join(RECOVERY_JOURNAL_FILE);
        let applied = execute_apply(plan, &journal_path, &ScanCancellation::new(), |_| {});
        assert_eq!(applied.receipt.status(), RenameTransactionStatus::Applied);

        let mut interrupted = applied.receipt;
        let entry = &mut interrupted.entries[0];
        let previous_stage = entry.stage_path.clone();
        let pending_stage = entry.parent_path.join(".mlrn-0123456789abcdef.tmp");
        assert_ne!(previous_stage, pending_stage);
        assert!(!pending_stage.exists());
        std::fs::rename(&entry.final_path, &previous_stage).unwrap();
        entry.current_path = Some(previous_stage.clone());
        entry.stage_paths = vec![previous_stage.clone(), pending_stage.clone()];
        interrupted.status = RenameTransactionStatus::Applying;
        interrupted.pending_move = Some(PendingMove {
            source_ref: entry.source_ref,
            from: previous_stage.clone(),
            to: pending_stage.clone(),
            identity: entry.identity,
            content_sha256: entry.content_sha256,
        });
        let before_reopen = RecoveryJournal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            receipts: vec![interrupted],
        };
        save_journal(&journal_path, &before_reopen).unwrap();

        let receipts = load_and_reconcile_journal(&journal_path, &ScanCancellation::new()).unwrap();
        let recovered = &receipts[0];
        assert_eq!(
            recovered.status(),
            RenameTransactionStatus::RecoveryRequired
        );
        assert_eq!(
            recovered.entries()[0].current_path(),
            Some(previous_stage.as_path())
        );
        assert_eq!(recovered.entries()[0].identity, rows[0].identity);
        assert_eq!(std::fs::read(&previous_stage).unwrap(), b"staged payload");
        assert!(!pending_stage.exists());
        assert!(recovered.pending_move.is_none());
        assert_eq!(recovered.entries()[0].stage_paths, vec![previous_stage]);
    }

    #[cfg(windows)]
    #[test]
    fn m8d_windows_fixture_undo_refuses_same_identity_and_bytes_after_timestamp_change() {
        use std::time::Duration;

        let fixture = workspace();
        let root = create_root(&fixture, "changed-time-undo", &[("a.txt", b"same bytes")]);
        let (plan, rows) = verified_plan(&root, &["a.txt"], prefix_rules("new-"));
        let original_identity = rows[0].identity;
        let journal_path = fixture.path().join(RECOVERY_JOURNAL_FILE);
        let applied = execute_apply(plan, &journal_path, &ScanCancellation::new(), |_| {});
        assert_eq!(applied.receipt.status(), RenameTransactionStatus::Applied);

        let changed = root.join("new-a.txt");
        let original_modified = applied.receipt.entries()[0].expected_modified;
        let modified = original_modified + Duration::from_secs(3600);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&changed)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(identity_at(&root, "new-a.txt"), original_identity);
        assert_eq!(std::fs::read(&changed).unwrap(), b"same bytes");
        assert_ne!(
            std::fs::metadata(&changed).unwrap().modified().unwrap(),
            original_modified
        );

        let undo = execute_undo(
            applied.receipt.id(),
            &journal_path,
            &ScanCancellation::new(),
            |_| {},
        );
        assert_eq!(
            undo.receipt.status(),
            RenameTransactionStatus::RecoveryRequired
        );
        assert_eq!(std::fs::read(&changed).unwrap(), b"same bytes");
        assert_eq!(identity_at(&root, "new-a.txt"), original_identity);
        assert!(!root.join("a.txt").exists());
    }

    #[cfg(windows)]
    fn identity_at(root: &Path, name: &str) -> NativeFileIdentity {
        let result = file_operations::scan_rename_root(
            RenameScanRequest::new(
                ExplicitDirectoryRoot::new(root.to_path_buf()).unwrap(),
                false,
                RenameScanGeneration(999),
            ),
            &ScanCancellation::new(),
        );
        assert_eq!(result.status(), RenameScanStatus::Complete);
        result
            .rows()
            .iter()
            .find(|row| row.path().file_name() == Some(std::ffi::OsStr::new(name)))
            .unwrap_or_else(|| panic!("file {name} not found in {}", root.display()))
            .identity()
    }

    #[cfg(windows)]
    fn directory_names(root: &Path) -> Vec<String> {
        let mut names = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    }
}
