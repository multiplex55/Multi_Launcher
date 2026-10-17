//! Bounded Hot Folder action admission, execution receipts and loop control.
//!
//! The watcher owns only settled candidates. This module claims one candidate,
//! rechecks its current rule and native evidence, and delegates all mutation to
//! the shared native file-operation owner.

use super::capability::{ActionCapability, HotFolderCapabilityContext, evaluate_action_capability};
use super::model::{
    ConfigurationFingerprint, HotFolderRule, MAX_RULES, RuleAction, RuleId, RuleTrigger,
};
use super::runtime::{
    HotFolderCandidateId, HotFolderFileEvidence, HotFolderLeaseDisposition,
    HotFolderReadyCandidate, HotFolderRuntimeError, HotFolderWatchService,
};
use super::store::{HotFolderRulesSnapshot, HotFolderRulesStore};
use crate::batch_rename::{self, transform_filename_with_prefix_suffix};
use crate::file_operations::{
    NativeFileIdentity, NativeHotFolderAction, NativeHotFolderError, NativeHotFolderFailureKind,
    NativeHotFolderInput, NativeHotFolderOutput, NativeHotFolderOutputKind, NativeHotFolderPhase,
    NativeHotFolderProgress, NativeHotFolderResult, NativeWatchRoot, ScanCancellation, admit_watch_root,
    execute_hotfolder_native_action, fingerprint_hotfolder_file,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const JOURNAL_FILE: &str = "hotfolder_execution.json";
const JOURNAL_VERSION: u32 = 1;
const MAX_JOURNAL_BYTES: usize = 2 * 1024 * 1024;
const MAX_JOURNAL_ROWS: usize = 512;
const MAX_PENDING_JOURNAL_ROWS: usize = 64;
const MAX_JOURNAL_RUN_COUNTERS: usize = 4096;
const MAX_RETAINED_RECEIPTS: usize = 128;
const MAX_PROCESSED_KEYS: usize = 512;
const MAX_CAUSAL_OUTPUTS: usize = 256;
const MAX_ACTIVE_RUNS_PER_ENABLE: u32 = 512;
const MAX_GLOBAL_RUNS_PER_MINUTE: usize = 120;
const MAX_RULE_RUNS_PER_MINUTE: usize = 30;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const CAUSAL_RETENTION: Duration = Duration::from_secs(10 * 60);
const MAX_RECEIPT_REASON_CHARS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotFolderTerminalStatus {
    Success,
    Failure,
    Partial,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotFolderExecutionPhase {
    Admission,
    Staging,
    Published,
    SourceRemoval,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotFolderExecutionAction {
    Copy,
    Move,
    Rename,
    RegisteredLauncherAction,
    MkMacro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotFolderOutputKind {
    Final,
    OwnedStage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotFolderOutputEvidence {
    pub kind: HotFolderOutputKind,
    pub path: PathBuf,
    pub identity: NativeFileIdentity,
    pub length: u64,
    pub modified: Option<SystemTime>,
    pub sha256: Option<[u8; 32]>,
}

/// One actual terminal outcome. A ready candidate or lease is never exposed as
/// successful execution; callers receive this only after the action port has
/// completed or failed at a safe boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotFolderExecutionReceipt {
    pub operation_id: u128,
    pub candidate_id: HotFolderCandidateId,
    pub rule_id: RuleId,
    pub rule_fingerprint: ConfigurationFingerprint,
    pub action: HotFolderExecutionAction,
    pub source_path: PathBuf,
    pub source_identity: NativeFileIdentity,
    pub source_length: u64,
    pub source_modified: SystemTime,
    pub source_sha256: Option<[u8; 32]>,
    pub outputs: Vec<HotFolderOutputEvidence>,
    pub phase: HotFolderExecutionPhase,
    pub status: HotFolderTerminalStatus,
    pub started_unix_nanos: u128,
    pub duration_ms: u64,
    pub reason: Option<String>,
    pub durable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotFolderRunDisposition {
    Executed(HotFolderExecutionReceipt),
    SuppressedDuplicate,
    SuppressedOwnOutput {
        operation_id: u128,
        originating_candidate: HotFolderCandidateId,
    },
    Deferred {
        retry_after: Duration,
    },
    StaleCandidate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotFolderExecutionSnapshot {
    pub paused: bool,
    pub shutdown: bool,
    pub active: bool,
    pub completed_runs: u64,
    pub suppressed_duplicates: u64,
    pub suppressed_outputs: u64,
    pub global_throttled_until: Option<Instant>,
    pub tripped_rules: Vec<RuleId>,
    pub recovery_required: usize,
    pub receipts: Vec<HotFolderExecutionReceipt>,
    pub journal_error: Option<String>,
}

#[derive(Debug)]
pub enum HotFolderExecutionError {
    Watcher(HotFolderRuntimeError),
    Journal(std::io::Error),
    InvalidJournal,
}

impl From<HotFolderRuntimeError> for HotFolderExecutionError {
    fn from(error: HotFolderRuntimeError) -> Self {
        Self::Watcher(error)
    }
}

/// Correlated handoff for the later launcher/MkMacro dispatch milestone.
/// M10-C deliberately ships without a production adapter, so these actions
/// remain unavailable and are never invoked here.
#[derive(Clone, Debug)]
pub struct HotFolderExternalActionRequest {
    pub operation_id: u128,
    pub candidate_id: HotFolderCandidateId,
    pub rule_id: RuleId,
    pub rule_fingerprint: ConfigurationFingerprint,
    pub action: RuleAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotFolderExternalActionOutcome {
    pub status: HotFolderTerminalStatus,
    pub phase: HotFolderExecutionPhase,
    pub reason: Option<String>,
}

/// Narrow integration seam for M10-D. Production remains `None` through M10-C.
pub trait HotFolderExternalActionAdapter: Send + Sync {
    fn dispatch(
        &self,
        request: HotFolderExternalActionRequest,
        cancellation: &ScanCancellation,
    ) -> HotFolderExternalActionOutcome;
}

trait HotFolderOperationPort: Send + Sync {
    fn admit_root(&self, path: &Path) -> Result<NativeWatchRoot, NativeHotFolderError>;

    fn fingerprint(
        &self,
        root: &NativeWatchRoot,
        path: &Path,
        evidence: HotFolderFileEvidence,
        cancellation: &ScanCancellation,
    ) -> Result<NativeHotFolderInput, NativeHotFolderError>;

    fn execute(
        &self,
        root: &NativeWatchRoot,
        path: &Path,
        input: NativeHotFolderInput,
        action: NativeHotFolderAction,
        destination: Option<&NativeWatchRoot>,
        target_leaf: &OsStr,
        operation_id: u128,
        progress: &mut dyn FnMut(NativeHotFolderProgress) -> io::Result<()>,
        cancellation: &ScanCancellation,
    ) -> Result<NativeHotFolderResult, NativeHotFolderError>;
}

struct NativeHotFolderOperationPort;

impl HotFolderOperationPort for NativeHotFolderOperationPort {
    fn admit_root(&self, path: &Path) -> Result<NativeWatchRoot, NativeHotFolderError> {
        admit_watch_root(path).map_err(|_| {
            native_execution_error(
                NativeHotFolderFailureKind::DestinationUnavailable,
                "explicit directory could not be admitted",
                Vec::new(),
            )
        })
    }

    fn fingerprint(
        &self,
        root: &NativeWatchRoot,
        path: &Path,
        evidence: HotFolderFileEvidence,
        cancellation: &ScanCancellation,
    ) -> Result<NativeHotFolderInput, NativeHotFolderError> {
        fingerprint_hotfolder_file(
            root,
            path,
            evidence.root_identity,
            evidence.parent_identity,
            evidence.file_identity,
            evidence.length,
            evidence.modified,
            cancellation,
        )
    }

    fn execute(
        &self,
        root: &NativeWatchRoot,
        path: &Path,
        input: NativeHotFolderInput,
        action: NativeHotFolderAction,
        destination: Option<&NativeWatchRoot>,
        target_leaf: &OsStr,
        operation_id: u128,
        progress: &mut dyn FnMut(NativeHotFolderProgress) -> io::Result<()>,
        cancellation: &ScanCancellation,
    ) -> Result<NativeHotFolderResult, NativeHotFolderError> {
        execute_hotfolder_native_action(
            root,
            path,
            input,
            action,
            destination,
            target_leaf,
            operation_id,
            progress,
            cancellation,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ExecutionKey {
    rule_id: RuleId,
    rule_fingerprint: ConfigurationFingerprint,
    epoch: u64,
    path: PathBuf,
    parent_identity: NativeFileIdentity,
    file_identity: NativeFileIdentity,
    length: u64,
    modified_nanos: u128,
    sha256: Option<[u8; 32]>,
}

struct ActiveOperation {
    key: ExecutionKey,
    paths: Vec<PathBuf>,
    cancellation: ScanCancellation,
}

struct CausalOutput {
    operation_id: u128,
    candidate_id: HotFolderCandidateId,
    path: PathBuf,
    identity: NativeFileIdentity,
    length: u64,
    modified: Option<SystemTime>,
    sha256: Option<[u8; 32]>,
    expires: Instant,
}

struct EngineState {
    paused: bool,
    shutdown: bool,
    active: Option<ActiveOperation>,
    processed: VecDeque<ExecutionKey>,
    completed_runs: u64,
    suppressed_duplicates: u64,
    suppressed_outputs: u64,
    global_runs: VecDeque<Instant>,
    rule_runs: HashMap<(RuleId, u64), VecDeque<Instant>>,
    enable_runs: HashMap<(RuleId, String, u64), u32>,
    tripped_rules: HashSet<RuleId>,
    global_throttled_until: Option<Instant>,
    causal_outputs: VecDeque<CausalOutput>,
    receipts: VecDeque<HotFolderExecutionReceipt>,
    journal: ExecutionJournal,
    journal_error: Option<String>,
}

/// Bounded single-action executor. `run_next_ready` is intended for the
/// background lifecycle owner, never the per-frame GUI path.
pub struct HotFolderExecutionEngine {
    state: Mutex<EngineState>,
    journal_path: PathBuf,
    operation_port: Arc<dyn HotFolderOperationPort>,
    clock: Arc<dyn ExecutionClock>,
}

impl HotFolderExecutionEngine {
    pub fn new(store: &HotFolderRulesStore) -> Result<Self, HotFolderExecutionError> {
        Self::new_with_dependencies(
            store,
            Arc::new(NativeHotFolderOperationPort),
            Arc::new(SystemExecutionClock),
        )
    }

    fn new_with_dependencies(
        store: &HotFolderRulesStore,
        operation_port: Arc<dyn HotFolderOperationPort>,
        clock: Arc<dyn ExecutionClock>,
    ) -> Result<Self, HotFolderExecutionError> {
        let directory = store
            .path()
            .parent()
            .ok_or(HotFolderExecutionError::InvalidJournal)?;
        let journal_path = directory.join(JOURNAL_FILE);
        let journal = ExecutionJournal::load(&journal_path)?;
        let mut processed = VecDeque::new();
        let mut receipts = VecDeque::new();
        let enable_runs = journal
            .run_counters
            .iter()
            .map(|counter| {
                (
                    (
                        counter.rule_id.clone(),
                        counter.rule_fingerprint.as_str().to_owned(),
                        counter.epoch,
                    ),
                    counter.runs,
                )
            })
            .collect::<HashMap<_, _>>();
        let mut completed_runs = 0_u64;
        for row in &journal.rows {
            let key = row.key.to_key()?;
            push_bounded_key(&mut processed, key.clone());
            if let JournalState::Terminal(receipt) = &row.state {
                receipts.push_back(receipt.to_receipt()?);
                completed_runs = completed_runs.saturating_add(1);
            }
        }
        while receipts.len() > MAX_RETAINED_RECEIPTS {
            receipts.pop_front();
        }
        Ok(Self {
            state: Mutex::new(EngineState {
                paused: false,
                shutdown: false,
                active: None,
                processed,
                completed_runs,
                suppressed_duplicates: 0,
                suppressed_outputs: 0,
                global_runs: VecDeque::new(),
                rule_runs: HashMap::new(),
                enable_runs,
                tripped_rules: HashSet::new(),
                global_throttled_until: None,
                causal_outputs: VecDeque::new(),
                receipts,
                journal,
                journal_error: None,
            }),
            journal_path,
            operation_port,
            clock,
        })
    }

    pub fn pause(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.paused = true;
            if let Some(active) = &state.active {
                active.cancellation.cancel();
            }
        }
    }

    pub fn resume(&self) {
        if let Ok(mut state) = self.state.lock()
            && !state.shutdown
        {
            state.paused = false;
            state.tripped_rules.clear();
        }
    }

    pub fn cancel_rule(&self, rule_id: &RuleId) {
        if let Ok(mut state) = self.state.lock()
            && state
                .active
                .as_ref()
                .is_some_and(|active| &active.key.rule_id == rule_id)
        {
            if let Some(active) = &state.active {
                active.cancellation.cancel();
            }
            state.tripped_rules.insert(rule_id.clone());
        }
    }

    /// Reconfiguration cancels an in-flight action for any disabled, removed,
    /// or changed rule. The caller supplies a fresh store snapshot after every
    /// explicit enable/edit/delete transition.
    pub fn reconcile_rules(&self, snapshot: &HotFolderRulesSnapshot) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if let Some(active) = &state.active {
            let still_current = snapshot.rules.iter().any(|rule| {
                rule.id == active.key.rule_id
                    && rule.enabled
                    && ConfigurationFingerprint::of_serializable(rule)
                        .is_ok_and(|fingerprint| fingerprint == active.key.rule_fingerprint)
            });
            if !still_current {
                active.cancellation.cancel();
            }
        }
        let current = snapshot
            .rules
            .iter()
            .filter(|rule| rule.enabled)
            .map(|rule| rule.id.clone())
            .collect::<HashSet<_>>();
        state.tripped_rules.retain(|id| current.contains(id));
    }

    pub fn shutdown(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.shutdown = true;
            state.paused = true;
            if let Some(active) = &state.active {
                active.cancellation.cancel();
            }
        }
    }

    pub fn snapshot(&self) -> HotFolderExecutionSnapshot {
        let Ok(state) = self.state.lock() else {
            return HotFolderExecutionSnapshot {
                paused: true,
                shutdown: true,
                active: false,
                completed_runs: 0,
                suppressed_duplicates: 0,
                suppressed_outputs: 0,
                global_throttled_until: None,
                tripped_rules: Vec::new(),
                recovery_required: 0,
                receipts: Vec::new(),
                journal_error: Some("execution state is unavailable".into()),
            };
        };
        HotFolderExecutionSnapshot {
            paused: state.paused,
            shutdown: state.shutdown,
            active: state.active.is_some(),
            completed_runs: state.completed_runs,
            suppressed_duplicates: state.suppressed_duplicates,
            suppressed_outputs: state.suppressed_outputs,
            global_throttled_until: state.global_throttled_until,
            tripped_rules: state.tripped_rules.iter().cloned().collect(),
            recovery_required: state
                .journal
                .rows
                .iter()
                .filter(|row| matches!(&row.state, JournalState::Pending { .. }))
                .count(),
            receipts: state.receipts.iter().cloned().collect(),
            journal_error: state.journal_error.clone(),
        }
    }

    fn persist_native_progress(
        &self,
        key: &ExecutionKey,
        journal_row: &mut JournalRow,
        progress: NativeHotFolderProgress,
    ) -> io::Result<()> {
        let phase = match progress.phase {
            NativeHotFolderPhase::Admission => HotFolderExecutionPhase::Admission,
            NativeHotFolderPhase::Staging => HotFolderExecutionPhase::Staging,
            NativeHotFolderPhase::Published => HotFolderExecutionPhase::Published,
            NativeHotFolderPhase::SourceRemoval => HotFolderExecutionPhase::SourceRemoval,
            NativeHotFolderPhase::Complete => HotFolderExecutionPhase::Complete,
        };
        let outputs = progress
            .outputs
            .into_iter()
            .map(output_evidence)
            .map(|output| JournalOutputEvidence::from_output(&output))
            .collect();
        let next_state = JournalState::Pending { phase, outputs };
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("execution journal state is unavailable"))?;
        let index = state
            .journal
            .rows
            .iter()
            .position(|row| row.key.matches(key))
            .ok_or_else(|| io::Error::other("pending execution intent is unavailable"))?;
        let previous = state.journal.rows[index].clone();
        state.journal.rows[index].state = next_state.clone();
        journal_row.state = next_state;
        match state.journal.save(&self.journal_path) {
            Ok(()) => Ok(()),
            Err(error) => {
                state.journal.rows[index] = previous.clone();
                journal_row.state = previous.state;
                state.journal_error = Some("native mutation progress could not be persisted".into());
                Err(error)
            }
        }
    }

    fn record_admission_failure(
        &self,
        watcher: &HotFolderWatchService,
        lease: super::runtime::HotFolderReadyLease,
        candidate: &HotFolderReadyCandidate,
        rule: &HotFolderRule,
        fingerprint: &ConfigurationFingerprint,
        error: NativeHotFolderError,
    ) -> Result<HotFolderRunDisposition, HotFolderExecutionError> {
        if watcher
            .finish_lease(lease, HotFolderLeaseDisposition::TransferToConsumer)
            .is_err()
        {
            return Ok(HotFolderRunDisposition::StaleCandidate);
        }
        let key = ExecutionKey::from_candidate(candidate, None);
        let now = self.clock.now();
        let started_wall = self.clock.wall_now();
        let started = now;
        let operation_id = rand::random::<u128>();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| HotFolderExecutionError::InvalidJournal)?;
            if state.shutdown || state.paused || state.active.is_some() {
                return Ok(HotFolderRunDisposition::StaleCandidate);
            }
            if state
                .processed
                .iter()
                .any(|known| processed_suppression_matches(known, &key))
            {
                state.suppressed_duplicates = state.suppressed_duplicates.saturating_add(1);
                return Ok(HotFolderRunDisposition::SuppressedDuplicate);
            }
            state.active = Some(ActiveOperation {
                key: key.clone(),
                paths: vec![candidate.path.clone()],
                cancellation: ScanCancellation::new(),
            });
        }
        let (status, phase, outputs, reason) = terminal_from_native(Err(error));
        let receipt = HotFolderExecutionReceipt {
            operation_id,
            candidate_id: candidate.correlation,
            rule_id: rule.id.clone(),
            rule_fingerprint: fingerprint.clone(),
            action: execution_action(rule.action.kind_name()),
            source_path: candidate.path.clone(),
            source_identity: candidate.evidence.file_identity,
            source_length: candidate.evidence.length,
            source_modified: candidate.evidence.modified,
            source_sha256: None,
            outputs,
            phase,
            status,
            started_unix_nanos: system_time_nanos(started_wall),
            duration_ms: u64::try_from(self.clock.now().saturating_duration_since(started).as_millis())
                .unwrap_or(u64::MAX),
            reason: reason.map(sanitize_reason),
            durable: true,
        };
        let mut row = JournalRow::pending(
            JournalKey::from_key(&key),
            operation_id,
            receipt.action,
            &candidate.path,
            &candidate.path,
        );
        row.state = JournalState::Terminal(JournalReceipt::from_receipt(&receipt));
        let durable = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| HotFolderExecutionError::InvalidJournal)?;
            state.active = None;
            let previous_rows = state.journal.rows.clone();
            let durable = if reserve_journal_row_capacity(&mut state.journal.rows) {
                state.journal.rows.push(row);
                if state.journal.save(&self.journal_path).is_ok() {
                    true
                } else {
                    state.journal.rows = previous_rows;
                    state.journal_error =
                        Some("admission failure receipt could not be persisted".into());
                    state.tripped_rules.insert(rule.id.clone());
                    false
                }
            } else {
                state.journal.rows = previous_rows;
                state.journal_error =
                    Some("unresolved recovery entries fill the journal capacity".into());
                state.tripped_rules.insert(rule.id.clone());
                false
            };
            push_bounded_key(&mut state.processed, key);
            state.completed_runs = state.completed_runs.saturating_add(1);
            state.receipts.push_back(HotFolderExecutionReceipt {
                durable,
                ..receipt.clone()
            });
            while state.receipts.len() > MAX_RETAINED_RECEIPTS {
                state.receipts.pop_front();
            }
            durable
        };
        Ok(HotFolderRunDisposition::Executed(HotFolderExecutionReceipt {
            durable,
            ..receipt
        }))
    }

    /// Claim at most one settled B candidate, revalidate the current selected
    /// rule and native source version, persist an intent, then execute one
    /// built-in action. Duplicates and causal outputs are consumed or deferred
    /// without being reported as successful file operations.
    pub fn run_next_ready(
        &self,
        watcher: &HotFolderWatchService,
        store: &HotFolderRulesStore,
        context: &HotFolderCapabilityContext<'_>,
    ) -> Result<Option<HotFolderRunDisposition>, HotFolderExecutionError> {
        let now = self.clock.now();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| HotFolderExecutionError::InvalidJournal)?;
            if state
                .global_throttled_until
                .is_some_and(|deadline| deadline <= now)
            {
                state.global_throttled_until = None;
            }
            if state.shutdown || state.paused || state.active.is_some() {
                return Ok(None);
            }
            if state
                .global_throttled_until
                .is_some_and(|deadline| deadline > now)
            {
                return Ok(None);
            }
        }
        let Some(lease) = watcher.try_lease_ready() else {
            return Ok(None);
        };
        let candidate = lease.candidate().clone();
        let rules = store.snapshot();

        // A disable/edit/delete is rejected against the store's current
        // in-memory revision. The lease transfer below separately verifies
        // that B still owns this exact active epoch and enable authorization.
        let Some(current_rule) = rules.rules.iter().find(|rule| rule.id == candidate.rule.id)
        else {
            watcher.finish_lease(lease, HotFolderLeaseDisposition::RejectStale)?;
            return Ok(Some(HotFolderRunDisposition::StaleCandidate));
        };
        let current_fingerprint = ConfigurationFingerprint::of_serializable(current_rule)
            .map_err(|_| HotFolderExecutionError::InvalidJournal)?;
        if !current_rule.enabled
            || current_fingerprint != candidate.rule_fingerprint
            || candidate.rule_fingerprint
                != ConfigurationFingerprint::of_serializable(&candidate.rule)
                    .map_err(|_| HotFolderExecutionError::InvalidJournal)?
            || !matches!(
                evaluate_action_capability(current_rule, context),
                ActionCapability::Supported { .. }
            )
        {
            watcher.finish_lease(lease, HotFolderLeaseDisposition::RejectStale)?;
            return Ok(Some(HotFolderRunDisposition::StaleCandidate));
        }

        let first = self.first_matching_rule(&rules, &candidate)?;
        if first.as_ref().is_none_or(|rule| rule.id != current_rule.id) {
            watcher.finish_lease(lease, HotFolderLeaseDisposition::Suppress)?;
            return Ok(Some(HotFolderRunDisposition::SuppressedDuplicate));
        }

        let root = match self.operation_port.admit_root(&current_rule.watched_root) {
            Ok(root) if root.identity() == candidate.evidence.root_identity => root,
            _ => {
                watcher.finish_lease(lease, HotFolderLeaseDisposition::RejectStale)?;
                return Ok(Some(HotFolderRunDisposition::StaleCandidate));
            }
        };
        let cancellation = ScanCancellation::new();
        let input = match self.operation_port.fingerprint(
            &root,
            &candidate.path,
            candidate.evidence,
            &cancellation,
        ) {
            Ok(input) => input,
            Err(error) if error.kind == NativeHotFolderFailureKind::StaleSource => {
                watcher.finish_lease(lease, HotFolderLeaseDisposition::RejectStale)?;
                return Ok(Some(HotFolderRunDisposition::StaleCandidate));
            }
            Err(error) => {
                return self
                    .record_admission_failure(
                        watcher,
                        lease,
                        &candidate,
                        current_rule,
                        &current_fingerprint,
                        error,
                    )
                    .map(Some);
            }
        };
        let key = ExecutionKey::from_candidate(&candidate, Some(input.sha256));
        let plan = self.plan_action(current_rule, &candidate, &root);
        let (action, destination, target_leaf, target_path) = match plan {
            Ok(plan) => plan,
            Err(error) => (
                PlannedAction::Rejected(error),
                None,
                candidate
                    .path
                    .file_name()
                    .unwrap_or_else(|| OsStr::new(""))
                    .to_os_string(),
                candidate.path.clone(),
            ),
        };
        let operation_id = rand::random::<u128>();
        let stage_path = destination.as_ref().map(|directory| {
            directory
                .canonical_root()
                .join(format!(".ml-hotfolder-{operation_id:032x}.stage"))
        });
        let mut active_paths = vec![candidate.path.clone(), target_path.clone()];
        if let Some(path) = stage_path {
            active_paths.push(path);
        }
        let started_wall = self.clock.wall_now();
        let started = self.clock.now();
        let action_kind = execution_action(current_rule.action.kind_name());
        let mut journal_row = JournalRow::pending(
            JournalKey::from_key(&key),
            operation_id,
            action_kind,
            &candidate.path,
            &target_path,
        );

        let decision = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| HotFolderExecutionError::InvalidJournal)?;
            let rule_key = (
                current_rule.id.clone(),
                current_fingerprint.as_str().to_owned(),
                candidate.epoch,
            );
            let rate_key = (current_rule.id.clone(), candidate.epoch);
            let mut decision = None;
            if state.shutdown || state.paused || state.active.is_some() {
                drop(state);
                drop(lease);
                return Ok(None);
            } else if state
                .processed
                .iter()
                .any(|known| processed_suppression_matches(known, &key))
            {
                state.suppressed_duplicates = state.suppressed_duplicates.saturating_add(1);
                decision = Some(HotFolderRunDisposition::SuppressedDuplicate);
            } else if state.tripped_rules.contains(&current_rule.id) {
                state.suppressed_duplicates = state.suppressed_duplicates.saturating_add(1);
                decision = Some(HotFolderRunDisposition::SuppressedDuplicate);
            } else if let Some((operation_id, candidate_id)) = state
                .causal_outputs
                .iter()
                .find(|output| {
                    output.expires > now
                        && paths_equivalent(&output.path, &candidate.path)
                        && output.identity == input.file_identity
                        && output.length == input.length
                        && output.modified == Some(input.modified)
                        && output.sha256 == Some(input.sha256)
                })
                .map(|output| (output.operation_id, output.candidate_id))
            {
                state.suppressed_outputs = state.suppressed_outputs.saturating_add(1);
                push_bounded_key(&mut state.processed, key.clone());
                decision = Some(HotFolderRunDisposition::SuppressedOwnOutput {
                    operation_id,
                    originating_candidate: candidate_id,
                });
            } else {
                let per_enable = state
                    .enable_runs
                    .get(&rule_key)
                    .copied()
                    .unwrap_or_default();
                if per_enable >= MAX_ACTIVE_RUNS_PER_ENABLE {
                    state.tripped_rules.insert(current_rule.id.clone());
                    decision = Some(HotFolderRunDisposition::SuppressedDuplicate);
                } else {
                    prune_rate_window(&mut state.global_runs, now);
                    let global_deadline = (state.global_runs.len() >= MAX_GLOBAL_RUNS_PER_MINUTE)
                        .then(|| state.global_runs.front().copied().unwrap_or(now) + RATE_WINDOW);
                    let rule_runs = state.rule_runs.entry(rate_key.clone()).or_default();
                    prune_rate_window(rule_runs, now);
                    let rule_deadline = (rule_runs.len() >= MAX_RULE_RUNS_PER_MINUTE)
                        .then(|| rule_runs.front().copied().unwrap_or(now) + RATE_WINDOW);
                    if let Some(deadline) = global_deadline.or(rule_deadline) {
                        state.global_throttled_until = Some(deadline);
                        decision = Some(HotFolderRunDisposition::Deferred {
                            retry_after: deadline.saturating_duration_since(now),
                        });
                    } else {
                        let pending_count = state
                            .journal
                            .rows
                            .iter()
                            .filter(|row| matches!(&row.state, JournalState::Pending { .. }))
                            .count();
                        if pending_count >= MAX_PENDING_JOURNAL_ROWS {
                            state.tripped_rules.insert(current_rule.id.clone());
                            state.journal_error =
                                Some("unresolved recovery entries fill the pending limit".into());
                            decision = Some(HotFolderRunDisposition::SuppressedDuplicate);
                        }
                    }
                }
            }
            if decision.is_none() {
                state.active = Some(ActiveOperation {
                    key: key.clone(),
                    paths: active_paths,
                    cancellation: cancellation.clone(),
                });
            }
            decision
        };
        if let Some(disposition) = decision {
            let lease_disposition =
                if matches!(disposition, HotFolderRunDisposition::Deferred { .. }) {
                    HotFolderLeaseDisposition::Defer
                } else {
                    HotFolderLeaseDisposition::Suppress
                };
            watcher.finish_lease(lease, lease_disposition)?;
            return Ok(Some(disposition));
        }

        // The lease transfer verifies B still owns this exact active epoch and
        // enable authorization. No file mutation has happened yet.
        if watcher
            .finish_lease(lease, HotFolderLeaseDisposition::TransferToConsumer)
            .is_err()
        {
            cancellation.cancel();
            if let Ok(mut state) = self.state.lock() {
                state.active = None;
            }
            return Ok(Some(HotFolderRunDisposition::StaleCandidate));
        }

        // Catch store edits that raced the initial read and lease transfer.
        // Lifecycle callers also call reconcile_rules on transitions so an
        // already-running cancellable operation is stopped promptly.
        let latest_rules = store.snapshot();
        let latest_rule_is_current = latest_rules.rules.iter().any(|rule| {
            rule.id == current_rule.id
                && rule.enabled
                && ConfigurationFingerprint::of_serializable(rule)
                    .is_ok_and(|fingerprint| fingerprint == current_fingerprint)
        });
        if !latest_rule_is_current {
            if let Ok(mut state) = self.state.lock() {
                state.active = None;
            }
            cancellation.cancel();
            return Ok(Some(HotFolderRunDisposition::StaleCandidate));
        }

        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| HotFolderExecutionError::InvalidJournal)?;
            if state.shutdown || state.paused || cancellation.is_cancelled() {
                state.active = None;
                return Ok(Some(HotFolderRunDisposition::StaleCandidate));
            }
            let rule_key = (
                current_rule.id.clone(),
                current_fingerprint.as_str().to_owned(),
                candidate.epoch,
            );
            let rate_key = (current_rule.id.clone(), candidate.epoch);
            // A fresh durable intent precedes every filesystem mutation. If it
            // cannot be saved, consume the candidate and leave the input alone.
            let previous_rows = state.journal.rows.clone();
            if !reserve_journal_row_capacity(&mut state.journal.rows) {
                state.journal.rows = previous_rows;
                state.tripped_rules.insert(current_rule.id.clone());
                state.active = None;
                state.journal_error =
                    Some("unresolved recovery entries fill the journal capacity".into());
                return Err(HotFolderExecutionError::InvalidJournal);
            }
            state.journal.rows.push(journal_row.clone());
            let counter_index = state.journal.run_counters.iter().position(|counter| {
                counter.rule_id == current_rule.id
                    && counter.rule_fingerprint == current_fingerprint
                    && counter.epoch == candidate.epoch
            });
            let old_counter = counter_index.map(|index| state.journal.run_counters[index].runs);
            if counter_index.is_none()
                && state.journal.run_counters.len() >= MAX_JOURNAL_RUN_COUNTERS
            {
                state.journal.rows = previous_rows;
                state.tripped_rules.insert(current_rule.id.clone());
                state.active = None;
                state.journal_error = Some("run budget journal capacity is exhausted".into());
                return Err(HotFolderExecutionError::InvalidJournal);
            }
            match counter_index {
                Some(index) => {
                    state.journal.run_counters[index].runs =
                        state.journal.run_counters[index].runs.saturating_add(1);
                }
                None => state.journal.run_counters.push(JournalRunCounter {
                    rule_id: current_rule.id.clone(),
                    rule_fingerprint: current_fingerprint.clone(),
                    epoch: candidate.epoch,
                    runs: 1,
                }),
            }
            if let Err(error) = state.journal.save(&self.journal_path) {
                state.journal.rows = previous_rows;
                if let Some(index) = counter_index {
                    if let Some(runs) = old_counter {
                        state.journal.run_counters[index].runs = runs;
                    }
                } else {
                    state.journal.run_counters.pop();
                }
                state.journal_error =
                    Some("could not persist the pre-action recovery intent".into());
                state.active = None;
                return Err(HotFolderExecutionError::Journal(error));
            }
            let enable_runs = state
                .enable_runs
                .get(&rule_key)
                .copied()
                .unwrap_or_default();
            state
                .enable_runs
                .insert(rule_key.clone(), enable_runs.saturating_add(1));
            state.global_runs.push_back(started);
            state
                .rule_runs
                .entry(rate_key)
                .or_default()
                .push_back(started);
        }

        let operation_result = match action {
            PlannedAction::Builtin {
                action,
                destination,
            } => {
                let mut record_progress = |progress| {
                    self.persist_native_progress(&key, &mut journal_row, progress)
                };
                self.operation_port.execute(
                    &root,
                    &candidate.path,
                    input,
                    action,
                    destination.as_ref(),
                    &target_leaf,
                    operation_id,
                    &mut record_progress,
                    &cancellation,
                )
            }
            PlannedAction::ExternalUnavailable => Err(native_execution_error(
                NativeHotFolderFailureKind::Io,
                "external action dispatch is unavailable until M10-D",
                Vec::new(),
            )),
            PlannedAction::Rejected(error) => Err(error),
        };
        let finished = self.clock.now();
        let (status, phase, outputs, reason) = terminal_from_native(operation_result);
        let receipt = HotFolderExecutionReceipt {
            operation_id,
            candidate_id: candidate.correlation,
            rule_id: current_rule.id.clone(),
            rule_fingerprint: current_fingerprint,
            action: action_kind,
            source_path: candidate.path.clone(),
            source_identity: input.file_identity,
            source_length: input.length,
            source_modified: input.modified,
            source_sha256: Some(input.sha256),
            outputs,
            phase,
            status,
            started_unix_nanos: system_time_nanos(started_wall),
            duration_ms: u64::try_from(finished.saturating_duration_since(started).as_millis())
                .unwrap_or(u64::MAX),
            reason: reason.map(sanitize_reason),
            durable: true,
        };
        journal_row.state = JournalState::Terminal(JournalReceipt::from_receipt(&receipt));
        let durable = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| HotFolderExecutionError::InvalidJournal)?;
            state.active = None;
            let rule_id = current_rule.id.clone();
            state.tripped_rules.remove(&rule_id);
            replace_journal_row(&mut state.journal.rows, &key, journal_row);
            let durable = state.journal.save(&self.journal_path).is_ok();
            if !durable {
                state.journal_error = Some("terminal receipt could not be persisted".into());
            }
            state.completed_runs = state.completed_runs.saturating_add(1);
            push_bounded_key(&mut state.processed, key.clone());
            state.receipts.push_back(HotFolderExecutionReceipt {
                durable,
                ..receipt.clone()
            });
            while state.receipts.len() > MAX_RETAINED_RECEIPTS {
                state.receipts.pop_front();
            }
            for output in &receipt.outputs {
                state.causal_outputs.push_back(CausalOutput {
                    operation_id,
                    candidate_id: candidate.correlation,
                    path: output.path.clone(),
                    identity: output.identity,
                    length: output.length,
                    modified: output.modified,
                    sha256: output.sha256,
                    expires: self.clock.now() + CAUSAL_RETENTION,
                });
            }
            while state.causal_outputs.len() > MAX_CAUSAL_OUTPUTS {
                state.causal_outputs.pop_front();
            }
            state
                .causal_outputs
                .retain(|output| output.expires > self.clock.now());
            durable
        };
        Ok(Some(HotFolderRunDisposition::Executed(
            HotFolderExecutionReceipt { durable, ..receipt },
        )))
    }

    fn first_matching_rule(
        &self,
        snapshot: &HotFolderRulesSnapshot,
        candidate: &HotFolderReadyCandidate,
    ) -> Result<Option<HotFolderRule>, HotFolderExecutionError> {
        for rule in snapshot.rules.iter().take(MAX_RULES) {
            if !rule.enabled || rule.trigger != candidate.rule.trigger || rule.validate().is_err() {
                continue;
            }
            let Ok(root) = self.operation_port.admit_root(&rule.watched_root) else {
                continue;
            };
            if !candidate_path_matches_rule(&candidate.path, &root, rule) {
                continue;
            }
            return Ok(Some(rule.clone()));
        }
        Ok(None)
    }

    fn plan_action(
        &self,
        rule: &HotFolderRule,
        candidate: &HotFolderReadyCandidate,
        source_root: &NativeWatchRoot,
    ) -> Result<(PlannedAction, Option<NativeWatchRoot>, OsString, PathBuf), NativeHotFolderError>
    {
        let source_leaf = candidate.path.file_name().ok_or_else(|| {
            native_execution_error(
                NativeHotFolderFailureKind::StaleSource,
                "source no longer has a filename",
                Vec::new(),
            )
        })?;
        match &rule.action {
            RuleAction::Copy { destination } | RuleAction::Move { destination } => {
                let destination_root = self.operation_port.admit_root(destination)?;
                let action = if matches!(rule.action, RuleAction::Copy { .. }) {
                    NativeHotFolderAction::Copy
                } else {
                    NativeHotFolderAction::Move
                };
                let target = destination_root.canonical_root().join(source_leaf);
                Ok((
                    PlannedAction::Builtin {
                        action,
                        destination: Some(destination_root.clone()),
                    },
                    Some(destination_root),
                    source_leaf.to_os_string(),
                    target,
                ))
            }
            RuleAction::Rename { prefix, suffix } => {
                let leaf = transform_filename_with_prefix_suffix(source_leaf, prefix, suffix)
                    .map_err(|_| {
                        native_execution_error(
                            NativeHotFolderFailureKind::UnsupportedFidelity,
                            "rename did not produce one safe filename",
                            Vec::new(),
                        )
                    })?;
                let target_leaf = OsString::from(leaf);
                let target = candidate
                    .path
                    .parent()
                    .ok_or_else(|| {
                        native_execution_error(
                            NativeHotFolderFailureKind::StaleSource,
                            "source parent directory is unavailable",
                            Vec::new(),
                        )
                    })?
                    .join(&target_leaf);
                let _ = source_root;
                Ok((
                    PlannedAction::Builtin {
                        action: NativeHotFolderAction::Rename,
                        destination: None,
                    },
                    None,
                    target_leaf,
                    target,
                ))
            }
            RuleAction::RegisteredLauncherAction { .. } | RuleAction::MkMacro { .. } => Ok((
                PlannedAction::ExternalUnavailable,
                None,
                source_leaf.to_os_string(),
                candidate.path.clone(),
            )),
        }
    }
}

impl Drop for HotFolderExecutionEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

enum PlannedAction {
    Builtin {
        action: NativeHotFolderAction,
        destination: Option<NativeWatchRoot>,
    },
    ExternalUnavailable,
    Rejected(NativeHotFolderError),
}

trait ExecutionClock: Send + Sync {
    fn now(&self) -> Instant;
    fn wall_now(&self) -> SystemTime;
}

struct SystemExecutionClock;

impl ExecutionClock for SystemExecutionClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wall_now(&self) -> SystemTime {
        SystemTime::now()
    }
}

fn candidate_path_matches_rule(path: &Path, root: &NativeWatchRoot, rule: &HotFolderRule) -> bool {
    let canonical_root = root.canonical_root();
    if !batch_rename::path_is_within_windows_ordinal(canonical_root, path)
        || batch_rename::compare_paths_windows_ordinal(canonical_root, path)
            == std::cmp::Ordering::Equal
    {
        return false;
    }
    let relative_depth = path
        .components()
        .count()
        .saturating_sub(canonical_root.components().count());
    if !rule.include_subfolders && relative_depth != 1 {
        return false;
    }
    let Some(filename) = path.file_name() else {
        return false;
    };
    if !rule.include_temporary_files && is_temporary_filename(filename) {
        return false;
    }
    match filename.to_str() {
        Some(filename) => rule.filter.matches_filename(filename),
        None => rule.filter.glob.is_none() && rule.filter.extensions.is_empty(),
    }
}

fn is_temporary_filename(filename: &OsStr) -> bool {
    filename.to_str().is_some_and(|filename| {
        [".part", ".crdownload"].iter().any(|suffix| {
            filename
                .get(filename.len().saturating_sub(suffix.len())..)
                .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
        })
    })
}

fn execution_action(kind: &str) -> HotFolderExecutionAction {
    match kind {
        "copy" => HotFolderExecutionAction::Copy,
        "move" => HotFolderExecutionAction::Move,
        "rename" => HotFolderExecutionAction::Rename,
        "mkmacro" => HotFolderExecutionAction::MkMacro,
        _ => HotFolderExecutionAction::RegisteredLauncherAction,
    }
}

fn terminal_from_native(
    result: Result<NativeHotFolderResult, NativeHotFolderError>,
) -> (
    HotFolderTerminalStatus,
    HotFolderExecutionPhase,
    Vec<HotFolderOutputEvidence>,
    Option<String>,
) {
    match result {
        Ok(result) => {
            let status = if result.partial {
                HotFolderTerminalStatus::Partial
            } else {
                HotFolderTerminalStatus::Success
            };
            let phase = execution_phase_from_native(result.phase);
            (
                status,
                phase,
                result.outputs.into_iter().map(output_evidence).collect(),
                result.reason.map(str::to_owned),
            )
        }
        Err(error) => {
            let status = if !error.retained_outputs.is_empty()
                || matches!(
                    error.phase,
                    NativeHotFolderPhase::Published | NativeHotFolderPhase::SourceRemoval
                )
            {
                HotFolderTerminalStatus::Partial
            } else if error.kind == NativeHotFolderFailureKind::Cancelled {
                HotFolderTerminalStatus::Cancelled
            } else {
                HotFolderTerminalStatus::Failure
            };
            let phase = execution_phase_from_native(error.phase);
            let outputs = error
                .retained_outputs
                .into_iter()
                .map(output_evidence)
                .collect();
            (status, phase, outputs, Some(error.reason.into()))
        }
    }
}

fn execution_phase_from_native(phase: NativeHotFolderPhase) -> HotFolderExecutionPhase {
    match phase {
        NativeHotFolderPhase::Admission => HotFolderExecutionPhase::Admission,
        NativeHotFolderPhase::Staging => HotFolderExecutionPhase::Staging,
        NativeHotFolderPhase::Published => HotFolderExecutionPhase::Published,
        NativeHotFolderPhase::SourceRemoval => HotFolderExecutionPhase::SourceRemoval,
        NativeHotFolderPhase::Complete => HotFolderExecutionPhase::Complete,
    }
}

fn output_evidence(output: NativeHotFolderOutput) -> HotFolderOutputEvidence {
    HotFolderOutputEvidence {
        kind: match output.kind {
            NativeHotFolderOutputKind::Final => HotFolderOutputKind::Final,
            NativeHotFolderOutputKind::OwnedStage => HotFolderOutputKind::OwnedStage,
        },
        path: output.path,
        identity: output.identity,
        length: output.length,
        modified: output.modified,
        sha256: output.sha256,
    }
}

fn native_execution_error(
    kind: NativeHotFolderFailureKind,
    reason: &'static str,
    retained_outputs: Vec<NativeHotFolderOutput>,
) -> NativeHotFolderError {
    NativeHotFolderError {
        kind,
        phase: match kind {
            NativeHotFolderFailureKind::StaleSource
            | NativeHotFolderFailureKind::Inaccessible
            | NativeHotFolderFailureKind::DestinationUnavailable => NativeHotFolderPhase::Admission,
            NativeHotFolderFailureKind::PartialMove => NativeHotFolderPhase::SourceRemoval,
            NativeHotFolderFailureKind::DestinationCollision
            | NativeHotFolderFailureKind::UnsupportedFidelity
            | NativeHotFolderFailureKind::Cancelled
            | NativeHotFolderFailureKind::Io => NativeHotFolderPhase::Staging,
        },
        reason,
        retained_outputs,
    }
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    batch_rename::compare_paths_windows_ordinal(left, right) == std::cmp::Ordering::Equal
}

fn push_bounded_key(keys: &mut VecDeque<ExecutionKey>, key: ExecutionKey) {
    keys.retain(|existing| existing != &key);
    keys.push_back(key);
    while keys.len() > MAX_PROCESSED_KEYS {
        keys.pop_front();
    }
}

/// Processed suppression may bridge an unknown digest recorded at admission
/// failure, but durable key equality remains exact for journal replacement.
fn processed_suppression_matches(known: &ExecutionKey, candidate: &ExecutionKey) -> bool {
    known.rule_id == candidate.rule_id
        && known.rule_fingerprint == candidate.rule_fingerprint
        && known.epoch == candidate.epoch
        && paths_equivalent(&known.path, &candidate.path)
        && known.parent_identity == candidate.parent_identity
        && known.file_identity == candidate.file_identity
        && known.length == candidate.length
        && known.modified_nanos == candidate.modified_nanos
        && match (known.sha256, candidate.sha256) {
            (Some(known), Some(candidate)) => known == candidate,
            _ => true,
        }
}

fn prune_rate_window(runs: &mut VecDeque<Instant>, now: Instant) {
    while runs
        .front()
        .is_some_and(|started| now.saturating_duration_since(*started) >= RATE_WINDOW)
    {
        runs.pop_front();
    }
}

fn replace_journal_row(rows: &mut Vec<JournalRow>, key: &ExecutionKey, row: JournalRow) {
    if let Some(index) = rows.iter().position(|existing| existing.key.matches(key)) {
        rows[index] = row;
    } else {
        rows.push(row);
    }
    trim_journal_rows(rows);
}

fn reserve_journal_row_capacity(rows: &mut Vec<JournalRow>) -> bool {
    while rows.len() >= MAX_JOURNAL_ROWS {
        let Some(index) = rows
            .iter()
            .position(|row| matches!(&row.state, JournalState::Terminal(_)))
        else {
            return false;
        };
        rows.remove(index);
    }
    true
}

fn trim_journal_rows(rows: &mut Vec<JournalRow>) {
    while rows.len() > MAX_JOURNAL_ROWS {
        if let Some(index) = rows
            .iter()
            .position(|row| matches!(&row.state, JournalState::Terminal(_)))
        {
            rows.remove(index);
        } else {
            break;
        }
    }
}

fn system_time_nanos(value: SystemTime) -> u128 {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn system_time_from_nanos(value: u128) -> SystemTime {
    let seconds = u64::try_from(value / 1_000_000_000).unwrap_or(u64::MAX);
    let nanos = u32::try_from(value % 1_000_000_000).unwrap_or_default();
    UNIX_EPOCH
        .checked_add(Duration::new(seconds, nanos))
        .unwrap_or(UNIX_EPOCH)
}

fn sanitize_reason(reason: String) -> String {
    reason
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_RECEIPT_REASON_CHARS)
        .collect()
}

impl ExecutionKey {
    fn from_candidate(
        candidate: &HotFolderReadyCandidate,
        sha256: Option<[u8; 32]>,
    ) -> Self {
        Self {
            rule_id: candidate.rule.id.clone(),
            rule_fingerprint: candidate.rule_fingerprint.clone(),
            epoch: candidate.epoch,
            path: candidate.path.clone(),
            parent_identity: candidate.evidence.parent_identity,
            file_identity: candidate.evidence.file_identity,
            length: candidate.evidence.length,
            modified_nanos: system_time_nanos(candidate.evidence.modified),
            sha256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionJournal {
    format_version: u32,
    rows: Vec<JournalRow>,
    run_counters: Vec<JournalRunCounter>,
}

impl ExecutionJournal {
    fn empty() -> Self {
        Self {
            format_version: JOURNAL_VERSION,
            rows: Vec::new(),
            run_counters: Vec::new(),
        }
    }

    fn load(path: &Path) -> Result<Self, HotFolderExecutionError> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::empty()),
            Err(error) => return Err(HotFolderExecutionError::Journal(error)),
        };
        if file
            .metadata()
            .map_err(HotFolderExecutionError::Journal)?
            .len()
            > MAX_JOURNAL_BYTES as u64
        {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_JOURNAL_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(HotFolderExecutionError::Journal)?;
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        let journal: Self =
            serde_json::from_slice(&bytes).map_err(|_| HotFolderExecutionError::InvalidJournal)?;
        if journal.format_version != JOURNAL_VERSION
            || journal.rows.len() > MAX_JOURNAL_ROWS
            || journal.run_counters.len() > MAX_JOURNAL_RUN_COUNTERS
        {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        let mut pending_count = 0;
        let mut keys = HashSet::with_capacity(journal.rows.len());
        let mut counter_keys = HashSet::with_capacity(journal.run_counters.len());
        for counter in &journal.run_counters {
            if !counter.rule_id.is_valid()
                || !counter.rule_fingerprint.is_valid()
                || counter.epoch == 0
                || counter.runs > MAX_ACTIVE_RUNS_PER_ENABLE
                || !counter_keys.insert((
                    counter.rule_id.clone(),
                    counter.rule_fingerprint.as_str().to_owned(),
                    counter.epoch,
                ))
            {
                return Err(HotFolderExecutionError::InvalidJournal);
            }
        }
        for row in &journal.rows {
            let key = row.key.to_key()?;
            if !keys.insert(key)
                || !row
                    .source_path
                    .to_path()
                    .is_some_and(|path| path.is_absolute())
                || !row
                    .target_path
                    .to_path()
                    .is_some_and(|path| path.is_absolute())
            {
                return Err(HotFolderExecutionError::InvalidJournal);
            }
            match &row.state {
                JournalState::Pending { .. } => pending_count += 1,
                JournalState::Terminal(receipt) => {
                    let _ = receipt.to_receipt()?;
                }
            }
        }
        if pending_count > MAX_PENDING_JOURNAL_ROWS {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        Ok(journal)
    }

    fn save(&self, path: &Path) -> io::Result<()> {
        if self.format_version != JOURNAL_VERSION
            || self.rows.len() > MAX_JOURNAL_ROWS
            || self.run_counters.len() > MAX_JOURNAL_RUN_COUNTERS
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Hot Folder recovery journal exceeds its supported shape",
            ));
        }
        let bytes = serde_json::to_vec(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Hot Folder recovery journal exceeds its supported size",
            ));
        }
        crate::common::atomic_file::save_atomic(path, &bytes)
            .map_err(|error| io::Error::other(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalRow {
    key: JournalKey,
    #[serde(with = "journal_u128_decimal")]
    operation_id: u128,
    action: HotFolderExecutionAction,
    source_path: StoredPath,
    target_path: StoredPath,
    state: JournalState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalRunCounter {
    rule_id: RuleId,
    rule_fingerprint: ConfigurationFingerprint,
    epoch: u64,
    runs: u32,
}

impl JournalRow {
    fn pending(
        key: JournalKey,
        operation_id: u128,
        action: HotFolderExecutionAction,
        source_path: &Path,
        target_path: &Path,
    ) -> Self {
        Self {
            key,
            operation_id,
            action,
            source_path: StoredPath::from_path(source_path),
            target_path: StoredPath::from_path(target_path),
            state: JournalState::Pending {
                phase: HotFolderExecutionPhase::Admission,
                outputs: Vec::new(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalKey {
    rule_id: RuleId,
    rule_fingerprint: ConfigurationFingerprint,
    epoch: u64,
    path: StoredPath,
    parent_identity: NativeFileIdentity,
    file_identity: NativeFileIdentity,
    length: u64,
    #[serde(with = "journal_u128_decimal")]
    modified_nanos: u128,
    sha256: Option<[u8; 32]>,
}

impl JournalKey {
    fn from_key(key: &ExecutionKey) -> Self {
        Self {
            rule_id: key.rule_id.clone(),
            rule_fingerprint: key.rule_fingerprint.clone(),
            epoch: key.epoch,
            path: StoredPath::from_path(&key.path),
            parent_identity: key.parent_identity,
            file_identity: key.file_identity,
            length: key.length,
            modified_nanos: key.modified_nanos,
            sha256: key.sha256,
        }
    }

    fn to_key(&self) -> Result<ExecutionKey, HotFolderExecutionError> {
        if !self.rule_id.is_valid() || !self.rule_fingerprint.is_valid() || self.epoch == 0 {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        let path = self
            .path
            .to_path()
            .ok_or(HotFolderExecutionError::InvalidJournal)?;
        if !path.is_absolute() {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        Ok(ExecutionKey {
            rule_id: self.rule_id.clone(),
            rule_fingerprint: self.rule_fingerprint.clone(),
            epoch: self.epoch,
            path,
            parent_identity: self.parent_identity,
            file_identity: self.file_identity,
            length: self.length,
            modified_nanos: self.modified_nanos,
            sha256: self.sha256,
        })
    }

    fn matches(&self, key: &ExecutionKey) -> bool {
        Self::from_key(key) == *self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum JournalState {
    Pending {
        phase: HotFolderExecutionPhase,
        outputs: Vec<JournalOutputEvidence>,
    },
    Terminal(JournalReceipt),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum JournalOutputKind {
    Final,
    OwnedStage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalOutputEvidence {
    kind: JournalOutputKind,
    path: StoredPath,
    identity: NativeFileIdentity,
    length: u64,
    #[serde(with = "journal_option_u128_decimal")]
    modified_nanos: Option<u128>,
    sha256: Option<[u8; 32]>,
}

impl JournalOutputEvidence {
    fn from_output(output: &HotFolderOutputEvidence) -> Self {
        Self {
            kind: match output.kind {
                HotFolderOutputKind::Final => JournalOutputKind::Final,
                HotFolderOutputKind::OwnedStage => JournalOutputKind::OwnedStage,
            },
            path: StoredPath::from_path(&output.path),
            identity: output.identity,
            length: output.length,
            modified_nanos: output.modified.map(system_time_nanos),
            sha256: output.sha256,
        }
    }

    fn to_output(&self) -> Result<HotFolderOutputEvidence, HotFolderExecutionError> {
        let path = self
            .path
            .to_path()
            .ok_or(HotFolderExecutionError::InvalidJournal)?;
        if !path.is_absolute() {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        Ok(HotFolderOutputEvidence {
            kind: match self.kind {
                JournalOutputKind::Final => HotFolderOutputKind::Final,
                JournalOutputKind::OwnedStage => HotFolderOutputKind::OwnedStage,
            },
            path,
            identity: self.identity,
            length: self.length,
            modified: self.modified_nanos.map(system_time_from_nanos),
            sha256: self.sha256,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalReceipt {
    #[serde(with = "journal_u128_decimal")]
    operation_id: u128,
    candidate_id: u64,
    rule_id: RuleId,
    rule_fingerprint: ConfigurationFingerprint,
    action: HotFolderExecutionAction,
    source_path: StoredPath,
    source_identity: NativeFileIdentity,
    source_length: u64,
    #[serde(with = "journal_u128_decimal")]
    source_modified_nanos: u128,
    source_sha256: Option<[u8; 32]>,
    outputs: Vec<JournalOutputEvidence>,
    phase: HotFolderExecutionPhase,
    status: HotFolderTerminalStatus,
    #[serde(with = "journal_u128_decimal")]
    started_unix_nanos: u128,
    duration_ms: u64,
    reason: Option<String>,
    durable: bool,
}

impl JournalReceipt {
    fn from_receipt(receipt: &HotFolderExecutionReceipt) -> Self {
        Self {
            operation_id: receipt.operation_id,
            candidate_id: receipt.candidate_id.get(),
            rule_id: receipt.rule_id.clone(),
            rule_fingerprint: receipt.rule_fingerprint.clone(),
            action: receipt.action,
            source_path: StoredPath::from_path(&receipt.source_path),
            source_identity: receipt.source_identity,
            source_length: receipt.source_length,
            source_modified_nanos: system_time_nanos(receipt.source_modified),
            source_sha256: receipt.source_sha256,
            outputs: receipt
                .outputs
                .iter()
                .map(JournalOutputEvidence::from_output)
                .collect(),
            phase: receipt.phase,
            status: receipt.status,
            started_unix_nanos: receipt.started_unix_nanos,
            duration_ms: receipt.duration_ms,
            reason: receipt.reason.clone(),
            durable: receipt.durable,
        }
    }

    fn to_receipt(&self) -> Result<HotFolderExecutionReceipt, HotFolderExecutionError> {
        if !self.rule_id.is_valid()
            || !self.rule_fingerprint.is_valid()
            || self.outputs.len() > MAX_CAUSAL_OUTPUTS
            || self.reason.as_ref().is_some_and(|reason| {
                reason.len() > MAX_RECEIPT_REASON_CHARS || reason.chars().any(char::is_control)
            })
        {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        let source_path = self
            .source_path
            .to_path()
            .ok_or(HotFolderExecutionError::InvalidJournal)?;
        if !source_path.is_absolute() {
            return Err(HotFolderExecutionError::InvalidJournal);
        }
        Ok(HotFolderExecutionReceipt {
            operation_id: self.operation_id,
            candidate_id: HotFolderCandidateId::from_persisted(self.candidate_id),
            rule_id: self.rule_id.clone(),
            rule_fingerprint: self.rule_fingerprint.clone(),
            action: self.action,
            source_path,
            source_identity: self.source_identity,
            source_length: self.source_length,
            source_modified: system_time_from_nanos(self.source_modified_nanos),
            source_sha256: self.source_sha256,
            outputs: self
                .outputs
                .iter()
                .map(JournalOutputEvidence::to_output)
                .collect::<Result<_, _>>()?,
            phase: self.phase,
            status: self.status,
            started_unix_nanos: self.started_unix_nanos,
            duration_ms: self.duration_ms,
            reason: self.reason.clone(),
            durable: self.durable,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "encoding", content = "value", deny_unknown_fields)]
enum StoredPath {
    #[cfg(windows)]
    WindowsUtf16Le(String),
    #[cfg(unix)]
    UnixBytes(String),
    #[cfg(not(any(windows, unix)))]
    NativeText(String),
}

mod journal_u128_decimal {
    use super::parse_decimal_u128;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub(super) fn serialize<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        parse_decimal_u128(&encoded).map_err(D::Error::custom)
    }
}

mod journal_option_u128_decimal {
    use super::parse_decimal_u128;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub(super) fn serialize<S>(value: &Option<u128>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<u128>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)?
            .map(|encoded| parse_decimal_u128(&encoded).map_err(D::Error::custom))
            .transpose()
    }
}

fn parse_decimal_u128(encoded: &str) -> Result<u128, &'static str> {
    let bytes = encoded.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 39
        || (bytes.len() > 1 && bytes[0] == b'0')
        || !bytes.iter().all(u8::is_ascii_digit)
    {
        return Err("persisted unsigned 128-bit value is not canonical decimal");
    }
    encoded
        .parse()
        .map_err(|_| "persisted unsigned 128-bit value is out of range")
}

#[cfg(test)]
mod tests {
    use super::super::runtime::test_ready_candidate;
    use super::*;
    use crate::file_operations::test_native_watch_root;
    use crate::hotfolder::{HotFolderFileEvidence, HotFolderRule, RuleAction, RuleId};
    use crate::platform::app_data::AppDataRoot;
    use crate::universal_actions::{ActionResolutionContext, ActionSurface};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FixedClock {
        now: Instant,
        wall_now: SystemTime,
    }

    impl ExecutionClock for FixedClock {
        fn now(&self) -> Instant {
            self.now
        }

        fn wall_now(&self) -> SystemTime {
            self.wall_now
        }
    }

    struct FakeOperationPort {
        roots: HashMap<PathBuf, NativeWatchRoot>,
        execute_calls: AtomicUsize,
        journal_path: PathBuf,
        fingerprint_failure: Mutex<Option<NativeHotFolderError>>,
    }

    impl HotFolderOperationPort for FakeOperationPort {
        fn admit_root(&self, path: &Path) -> Result<NativeWatchRoot, NativeHotFolderError> {
            self.roots.get(path).cloned().ok_or_else(|| {
                native_execution_error(
                    NativeHotFolderFailureKind::DestinationUnavailable,
                    "fake root is unavailable",
                    Vec::new(),
                )
            })
        }

        fn fingerprint(
            &self,
            _root: &NativeWatchRoot,
            _path: &Path,
            evidence: HotFolderFileEvidence,
            _cancellation: &ScanCancellation,
        ) -> Result<NativeHotFolderInput, NativeHotFolderError> {
            if let Ok(mut failure) = self.fingerprint_failure.lock()
                && let Some(error) = failure.take()
            {
                return Err(error);
            }
            Ok(NativeHotFolderInput {
                root_identity: evidence.root_identity,
                parent_identity: evidence.parent_identity,
                file_identity: evidence.file_identity,
                length: evidence.length,
                modified: evidence.modified,
                sha256: [0x42; 32],
            })
        }

        fn execute(
            &self,
            _root: &NativeWatchRoot,
            source_path: &Path,
            input: NativeHotFolderInput,
            _action: NativeHotFolderAction,
            destination: Option<&NativeWatchRoot>,
            target_leaf: &OsStr,
            operation_id: u128,
            progress: &mut dyn FnMut(NativeHotFolderProgress) -> io::Result<()>,
            _cancellation: &ScanCancellation,
        ) -> Result<NativeHotFolderResult, NativeHotFolderError> {
            let call = self.execute_calls.fetch_add(1, Ordering::SeqCst);
            let output_root = destination
                .map(|root| root.canonical_root())
                .or_else(|| source_path.parent())
                .expect("fake action has an output root");
            let output = NativeHotFolderOutput {
                kind: NativeHotFolderOutputKind::Final,
                path: output_root.join(target_leaf),
                identity: test_native_identity(0x9000 + call as u64),
                length: input.length,
                modified: Some(input.modified),
                sha256: Some(input.sha256),
            };
            let stage_output = NativeHotFolderOutput {
                kind: NativeHotFolderOutputKind::OwnedStage,
                path: output_root.join(".m10c-stage-fixture"),
                identity: test_native_identity(0xA000 + call as u64),
                length: input.length,
                modified: Some(input.modified),
                sha256: Some(input.sha256),
            };
            for (phase, native_output, expected_phase) in [
                (
                    NativeHotFolderPhase::Staging,
                    stage_output,
                    HotFolderExecutionPhase::Staging,
                ),
                (
                    NativeHotFolderPhase::Published,
                    output.clone(),
                    HotFolderExecutionPhase::Published,
                ),
            ] {
                progress(NativeHotFolderProgress {
                    phase,
                    outputs: vec![native_output.clone()],
                })
                .expect("engine should persist native mutation progress");
                let persisted = ExecutionJournal::load(&self.journal_path)
                    .map_err(|_| {
                        native_execution_error(
                            NativeHotFolderFailureKind::Io,
                            "fixture journal could not be read",
                            Vec::new(),
                        )
                    })?;
                let Some(JournalRow {
                    state: JournalState::Pending { phase, outputs },
                    key,
                    operation_id: persisted_operation_id,
                    ..
                }) = persisted.rows.last()
                else {
                    return Err(native_execution_error(
                        NativeHotFolderFailureKind::Io,
                        "execution progress was not persisted before mutation advanced",
                        Vec::new(),
                    ));
                };
                assert_eq!(*phase, expected_phase);
                assert_eq!(*persisted_operation_id, operation_id);
                assert_eq!(key.modified_nanos, system_time_nanos(input.modified));
                assert_eq!(outputs.len(), 1);
                assert_eq!(outputs[0].identity, native_output.identity);
                assert_eq!(
                    outputs[0].modified_nanos,
                    native_output.modified.map(system_time_nanos)
                );
            }
            Err(NativeHotFolderError {
                kind: NativeHotFolderFailureKind::Io,
                phase: NativeHotFolderPhase::Published,
                reason: "simulated retained published output",
                retained_outputs: vec![output],
            })
        }
    }

    fn test_native_identity(id: u64) -> NativeFileIdentity {
        let mut file_id = [0; 16];
        file_id[..8].copy_from_slice(&id.to_le_bytes());
        crate::file_operations::test_native_file_identity(7, file_id)
    }

    fn enable_rule(
        store: &HotFolderRulesStore,
        snapshot: HotFolderRulesSnapshot,
        id: &RuleId,
    ) -> HotFolderRulesSnapshot {
        let mut rule = snapshot
            .rules
            .iter()
            .find(|rule| &rule.id == id)
            .expect("stored rule")
            .clone();
        rule.enabled = true;
        store
            .replace_rule(snapshot.revision, rule)
            .expect("enable rule")
    }

    fn ready_candidate(
        rule: &HotFolderRule,
        path: PathBuf,
        root_identity: NativeFileIdentity,
        parent_identity: NativeFileIdentity,
        file_id: u64,
        length: u64,
        revision: u64,
        correlation: u64,
    ) -> HotFolderReadyCandidate {
        let evidence = HotFolderFileEvidence {
            root_identity,
            parent_identity,
            file_identity: test_native_identity(file_id),
            length,
            modified: UNIX_EPOCH + Duration::from_secs(file_id),
        };
        let mut candidate = test_ready_candidate(rule.clone(), path, evidence, 1, revision);
        candidate.correlation = HotFolderCandidateId::from_persisted(correlation);
        candidate
    }

    fn run_candidate(
        engine: &HotFolderExecutionEngine,
        store: &HotFolderRulesStore,
        context: &HotFolderCapabilityContext<'_>,
        candidate: HotFolderReadyCandidate,
    ) -> Option<HotFolderRunDisposition> {
        let watcher = HotFolderWatchService::with_test_ready_candidate(candidate);
        engine
            .run_next_ready(&watcher, store, context)
            .expect("run candidate")
    }

    #[test]
    fn m10c_fake_ready_execution_dedupes_partial_outputs_and_caps_runs() {
        #[derive(Serialize, Deserialize)]
        struct DecimalU128Probe {
            #[serde(with = "super::journal_u128_decimal")]
            value: u128,
        }
        #[derive(Serialize, Deserialize)]
        struct OptionalDecimalU128Probe {
            #[serde(with = "super::journal_option_u128_decimal")]
            value: Option<u128>,
        }

        let max_u128 = u128::MAX;
        let encoded_max = serde_json::to_string(&DecimalU128Probe { value: max_u128 })
            .expect("serialize maximum u128 as a decimal string");
        assert_eq!(
            encoded_max,
            format!(r#"{{"value":"{max_u128}"}}"#)
        );
        assert_eq!(
            serde_json::from_str::<DecimalU128Probe>(&encoded_max)
                .expect("round-trip maximum u128")
                .value,
            max_u128
        );
        assert!(serde_json::from_str::<DecimalU128Probe>(r#"{"value":"01"}"#).is_err());
        assert!(serde_json::from_str::<DecimalU128Probe>(r#"{"value":1}"#).is_err());
        let encoded_optional = serde_json::to_string(&OptionalDecimalU128Probe {
            value: Some(max_u128),
        })
        .expect("serialize optional maximum u128 as a decimal string");
        assert_eq!(
            serde_json::from_str::<OptionalDecimalU128Probe>(&encoded_optional)
                .expect("round-trip optional maximum u128")
                .value,
            Some(max_u128)
        );
        assert_eq!(
            serde_json::from_str::<OptionalDecimalU128Probe>(r#"{"value":null}"#)
                .expect("round-trip absent optional u128")
                .value,
            None
        );

        let temp = tempfile::tempdir().expect("test workspace");
        let data_root =
            AppDataRoot::from_path(temp.path().join("target/productivity_expansion/app_data"));
        let store = HotFolderRulesStore::new(&data_root);
        store.load();

        let watched = temp
            .path()
            .join("target/productivity_expansion/watch/incoming");
        let nested = watched.join("nested");
        let destination = watched.join("output");
        let mut outer = HotFolderRule::new(
            "outer",
            &watched,
            RuleAction::Copy {
                destination: destination.clone(),
            },
        );
        outer.include_subfolders = true;
        let mut snapshot = store.add_rule(0, outer).expect("add outer rule");
        let outer_id = snapshot.rules[0].id.clone();
        snapshot = enable_rule(&store, snapshot, &outer_id);

        let inner = HotFolderRule::new(
            "inner",
            &nested,
            RuleAction::Copy {
                destination: destination.clone(),
            },
        );
        snapshot = store
            .add_rule(snapshot.revision, inner)
            .expect("add nested rule");
        let inner_id = snapshot.rules[1].id.clone();
        snapshot = enable_rule(&store, snapshot, &inner_id);
        let outer = snapshot.rules[0].clone();
        let inner = snapshot.rules[1].clone();

        let watched_root = test_native_watch_root(&watched, 7, test_native_identity(1).file_id());
        let nested_root = test_native_watch_root(&nested, 7, test_native_identity(2).file_id());
        let destination_root =
            test_native_watch_root(&destination, 7, test_native_identity(3).file_id());
        let fake = Arc::new(FakeOperationPort {
            roots: HashMap::from([
                (watched.clone(), watched_root.clone()),
                (nested.clone(), nested_root.clone()),
                (destination.clone(), destination_root.clone()),
            ]),
            execute_calls: AtomicUsize::new(0),
            journal_path: store
                .path()
                .parent()
                .expect("app-data parent")
                .join(JOURNAL_FILE),
            fingerprint_failure: Mutex::new(None),
        });
        let engine = HotFolderExecutionEngine::new_with_dependencies(
            &store,
            fake.clone(),
            Arc::new(FixedClock {
                now: Instant::now(),
                wall_now: UNIX_EPOCH + Duration::from_secs(100),
            }),
        )
        .expect("execution engine");
        let launcher_targets = [];
        let macro_document = crate::mkmacro::MkMacroDocument::default();
        let root_availability = HashMap::from([
            (
                watched.clone(),
                crate::hotfolder::RootAvailability::ExistingDirectory,
            ),
            (
                nested.clone(),
                crate::hotfolder::RootAvailability::ExistingDirectory,
            ),
            (
                destination.clone(),
                crate::hotfolder::RootAvailability::ExistingDirectory,
            ),
        ]);
        let context = HotFolderCapabilityContext::new(
            &launcher_targets,
            ActionResolutionContext::new(ActionSurface::LauncherList, ""),
            &macro_document,
            &root_availability,
        );

        let overlap = ready_candidate(
            &inner,
            nested.join("source.bin"),
            nested_root.identity(),
            nested_root.identity(),
            10,
            5,
            snapshot.revision,
            1,
        );
        assert_eq!(
            run_candidate(&engine, &store, &context, overlap),
            Some(HotFolderRunDisposition::SuppressedDuplicate)
        );
        assert_eq!(fake.execute_calls.load(Ordering::SeqCst), 0);

        snapshot = store
            .replace_rule(snapshot.revision, {
                let mut disabled = inner.clone();
                disabled.enabled = false;
                disabled
            })
            .expect("disable nested rule");
        let stale = ready_candidate(
            &inner,
            nested.join("source.bin"),
            nested_root.identity(),
            nested_root.identity(),
            11,
            5,
            snapshot.revision.saturating_sub(1),
            2,
        );
        assert_eq!(
            run_candidate(&engine, &store, &context, stale),
            Some(HotFolderRunDisposition::StaleCandidate)
        );

        let source_path = nested.join("source.bin");
        let source = ready_candidate(
            &outer,
            source_path.clone(),
            watched_root.identity(),
            nested_root.identity(),
            20,
            5,
            snapshot.revision,
            3,
        );
        let first = run_candidate(&engine, &store, &context, source.clone()).unwrap();
        let HotFolderRunDisposition::Executed(receipt) = first else {
            panic!("first matching rule should execute")
        };
        assert_eq!(
            receipt.status,
            HotFolderTerminalStatus::Partial,
            "reason={:?}, phase={:?}, outputs={:?}, execute_calls={}",
            receipt.reason,
            receipt.phase,
            receipt.outputs,
            fake.execute_calls.load(Ordering::SeqCst),
        );
        assert_eq!(receipt.phase, HotFolderExecutionPhase::Published);
        assert_eq!(receipt.outputs.len(), 1);
        assert!(receipt.durable);
        assert_eq!(fake.execute_calls.load(Ordering::SeqCst), 1);

        let persisted = ExecutionJournal::load(&fake.journal_path)
            .expect("load terminal execution receipt");
        let persisted_row = persisted
            .rows
            .iter()
            .find(|row| row.operation_id == receipt.operation_id)
            .expect("persisted operation ID");
        let JournalState::Terminal(persisted_receipt) = &persisted_row.state else {
            panic!("finished operation has a terminal receipt")
        };
        assert_eq!(persisted_row.operation_id, receipt.operation_id);
        assert_eq!(persisted_row.key.modified_nanos, system_time_nanos(receipt.source_modified));
        assert_eq!(persisted_receipt.operation_id, receipt.operation_id);
        assert_eq!(
            persisted_receipt.source_modified_nanos,
            system_time_nanos(receipt.source_modified)
        );
        assert_eq!(
            persisted_receipt.started_unix_nanos,
            receipt.started_unix_nanos
        );
        assert_eq!(
            persisted_receipt.outputs[0].modified_nanos,
            receipt.outputs[0].modified.map(system_time_nanos)
        );

        let reopened = HotFolderExecutionEngine::new_with_dependencies(
            &store,
            fake.clone(),
            Arc::new(FixedClock {
                now: Instant::now(),
                wall_now: UNIX_EPOCH + Duration::from_secs(100),
            }),
        )
        .expect("reopen execution journal");
        let reopened_receipt = reopened
            .snapshot()
            .receipts
            .into_iter()
            .find(|loaded| loaded.operation_id == receipt.operation_id)
            .expect("reopened exact terminal receipt");
        assert_eq!(reopened_receipt.source_modified, receipt.source_modified);
        assert_eq!(reopened_receipt.started_unix_nanos, receipt.started_unix_nanos);
        assert_eq!(reopened_receipt.outputs[0].modified, receipt.outputs[0].modified);

        let duplicate = run_candidate(&engine, &store, &context, source).unwrap();
        assert_eq!(duplicate, HotFolderRunDisposition::SuppressedDuplicate);
        assert_eq!(fake.execute_calls.load(Ordering::SeqCst), 1);

        let output = &receipt.outputs[0];
        let mut own_output = ready_candidate(
            &outer,
            output.path.clone(),
            watched_root.identity(),
            destination_root.identity(),
            0x9000,
            output.length,
            snapshot.revision,
            4,
        );
        own_output.evidence.modified = output.modified.expect("retained output timestamp");
        assert!(matches!(
            run_candidate(&engine, &store, &context, own_output),
            Some(HotFolderRunDisposition::SuppressedOwnOutput { .. })
        ));
        assert_eq!(fake.execute_calls.load(Ordering::SeqCst), 1);

        let edited_output = ready_candidate(
            &outer,
            output.path.clone(),
            watched_root.identity(),
            destination_root.identity(),
            0x9001,
            output.length + 1,
            snapshot.revision,
            5,
        );
        assert!(matches!(
            run_candidate(&engine, &store, &context, edited_output),
            Some(HotFolderRunDisposition::Executed(_))
        ));
        assert_eq!(fake.execute_calls.load(Ordering::SeqCst), 2);

        *fake.fingerprint_failure.lock().expect("fingerprint failure") = Some(
            NativeHotFolderError {
                phase: NativeHotFolderPhase::Admission,
                ..native_execution_error(
                    NativeHotFolderFailureKind::UnsupportedFidelity,
                    "source has named streams or its stream set cannot be verified",
                    Vec::new(),
                )
            },
        );
        let refused = ready_candidate(
            &outer,
            nested.join("stream-source.bin"),
            watched_root.identity(),
            nested_root.identity(),
            700,
            5,
            snapshot.revision,
            101,
        );
        let admission_key = ExecutionKey::from_candidate(&refused, None);
        assert!(processed_suppression_matches(
            &admission_key,
            &ExecutionKey::from_candidate(&refused, Some([0x42; 32]))
        ));
        assert!(!processed_suppression_matches(
            &ExecutionKey::from_candidate(&refused, Some([0x42; 32])),
            &ExecutionKey::from_candidate(&refused, Some([0x43; 32]))
        ));
        let mut changed_version = refused.clone();
        changed_version.evidence.length += 1;
        assert!(!processed_suppression_matches(
            &admission_key,
            &ExecutionKey::from_candidate(&changed_version, Some([0x42; 32]))
        ));
        let Some(HotFolderRunDisposition::Executed(refusal)) =
            run_candidate(&engine, &store, &context, refused.clone())
        else {
            panic!("unsupported source fidelity should have a terminal receipt")
        };
        assert_eq!(refusal.status, HotFolderTerminalStatus::Failure);
        assert_eq!(refusal.phase, HotFolderExecutionPhase::Admission);
        assert_eq!(refusal.source_sha256, None);
        assert!(refusal.reason.as_deref().is_some_and(|reason| reason.contains("named streams")));
        assert!(refusal.durable);
        assert_eq!(
            run_candidate(&engine, &store, &context, refused),
            Some(HotFolderRunDisposition::SuppressedDuplicate)
        );

        for index in 0..MAX_RULE_RUNS_PER_MINUTE - 2 {
            let candidate = ready_candidate(
                &outer,
                nested.join(format!("rate-{index}.bin")),
                watched_root.identity(),
                nested_root.identity(),
                100 + index as u64,
                5,
                snapshot.revision,
                10 + index as u64,
            );
            assert!(matches!(
                run_candidate(&engine, &store, &context, candidate),
                Some(HotFolderRunDisposition::Executed(_))
            ));
        }
        let capped = ready_candidate(
            &outer,
            nested.join("rate-capped.bin"),
            watched_root.identity(),
            nested_root.identity(),
            500,
            5,
            snapshot.revision,
            99,
        );
        assert!(matches!(
            run_candidate(&engine, &store, &context, capped),
            Some(HotFolderRunDisposition::Deferred { .. })
        ));
        assert_eq!(
            fake.execute_calls.load(Ordering::SeqCst),
            MAX_RULE_RUNS_PER_MINUTE
        );

        let terminal = engine
            .state
            .lock()
            .expect("execution state")
            .journal
            .rows
            .iter()
            .find(|row| matches!(&row.state, JournalState::Terminal(_)))
            .expect("one terminal row")
            .clone();
        let mut unresolved = terminal.clone();
        unresolved.state = JournalState::Pending {
            phase: HotFolderExecutionPhase::Published,
            outputs: Vec::new(),
        };
        let mut rows = vec![terminal; MAX_JOURNAL_ROWS - 1];
        rows.push(unresolved.clone());
        assert!(reserve_journal_row_capacity(&mut rows));
        assert_eq!(rows.len(), MAX_JOURNAL_ROWS - 1);
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(&row.state, JournalState::Pending { .. }))
                .count(),
            1
        );
        let mut unresolved_only = vec![unresolved; MAX_JOURNAL_ROWS];
        assert!(!reserve_journal_row_capacity(&mut unresolved_only));
        assert_eq!(unresolved_only.len(), MAX_JOURNAL_ROWS);

        assert!(engine.snapshot().global_throttled_until.is_some());

        engine.pause();
        let paused = ready_candidate(
            &outer,
            nested.join("paused.bin"),
            watched_root.identity(),
            nested_root.identity(),
            501,
            5,
            snapshot.revision,
            100,
        );
        assert_eq!(run_candidate(&engine, &store, &context, paused), None);
        assert_eq!(
            fake.execute_calls.load(Ordering::SeqCst),
            MAX_RULE_RUNS_PER_MINUTE
        );
        engine.shutdown();
    }
}

impl StoredPath {
    fn from_path(path: &Path) -> Self {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let bytes = path
                .as_os_str()
                .encode_wide()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            Self::WindowsUtf16Le(hex::encode(bytes))
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            Self::UnixBytes(hex::encode(path.as_os_str().as_bytes()))
        }
        #[cfg(not(any(windows, unix)))]
        {
            Self::NativeText(path.to_string_lossy().into_owned())
        }
    }

    fn to_path(&self) -> Option<PathBuf> {
        #[cfg(windows)]
        if let Self::WindowsUtf16Le(encoded) = self {
            use std::os::windows::ffi::OsStringExt;
            let bytes = hex::decode(encoded).ok()?;
            if bytes.len() % 2 != 0 || bytes.len() > 65_536 {
                return None;
            }
            let words = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>();
            return Some(PathBuf::from(OsString::from_wide(&words)));
        }
        #[cfg(unix)]
        if let Self::UnixBytes(encoded) = self {
            use std::os::unix::ffi::OsStringExt;
            let bytes = hex::decode(encoded).ok()?;
            if bytes.len() > 131_072 {
                return None;
            }
            return Some(PathBuf::from(OsString::from_vec(bytes)));
        }
        #[cfg(not(any(windows, unix)))]
        if let Self::NativeText(encoded) = self {
            if encoded.len() > 32_768 {
                return None;
            }
            return Some(PathBuf::from(encoded));
        }
        None
    }
}
