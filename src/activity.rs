use std::collections::{BTreeMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const TERMINAL_HISTORY_LIMIT: usize = 100;
pub const ACTIVE_FACTS_LIMIT: usize = 256;
const RETENTION_RECORDED: u8 = 0;
const RETENTION_OMITTED_ACTIVE: u8 = 1;
const RETENTION_OMITTED_TERMINAL: u8 = 2;
const RETENTION_TERMINAL_RETAINED: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActivityOwner {
    Indexing,
    FileSearch,
    Table,
    DuplicateFinder,
    Hash,
    Rename,
    SearchReplace,
    Xml,
    FolderCompare,
    HotFolder,
    Screenshot,
    Ocr,
    LivePin,
    Macro,
}

impl ActivityOwner {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Indexing => "Action indexing",
            Self::FileSearch => "File search",
            Self::Table => "Table utility",
            Self::DuplicateFinder => "Duplicate finder",
            Self::Hash => "Hash generator",
            Self::Rename => "Rename",
            Self::SearchReplace => "Search and replace",
            Self::Xml => "XML toolkit",
            Self::FolderCompare => "Folder comparison",
            Self::HotFolder => "Hot Folder",
            Self::Screenshot => "Screenshot capture",
            Self::Ocr => "OCR",
            Self::LivePin => "Live pin capture",
            Self::Macro => "Macro playback",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActivitySlot {
    Primary,
    Secondary,
    Preview,
    Export,
    LiveCapture,
    MacroRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActivityIncarnation(u64);

impl ActivityIncarnation {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActivityOperationId(u64);

impl ActivityOperationId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActivityKey {
    owner: ActivityOwner,
    incarnation: ActivityIncarnation,
    operation_id: ActivityOperationId,
}

impl ActivityKey {
    #[must_use]
    pub const fn owner(self) -> ActivityOwner {
        self.owner
    }

    #[must_use]
    pub const fn incarnation(self) -> ActivityIncarnation {
        self.incarnation
    }

    #[must_use]
    pub const fn operation_id(self) -> ActivityOperationId {
        self.operation_id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityOperation {
    IndexActions,
    SearchFiles,
    ParseTable,
    ProjectTable,
    ExportTable,
    ReadTableFile,
    FindDuplicates,
    CalculateHash,
    ScanRenameTargets,
    ApplyRename,
    UndoRename,
    SearchFilesForReplacement,
    PreviewReplacement,
    ConfirmReplacement,
    ReplaceFiles,
    FormatXml,
    ParseXml,
    InspectXmlNode,
    QueryXml,
    ValidateXmlSchema,
    LoadXmlFile,
    PrepareXmlSave,
    SaveXmlFile,
    CompareFolders,
    WatchHotFolder,
    CaptureScreenshot,
    CaptureOcr,
    CaptureLivePin,
    PlayMacro,
}

impl ActivityOperation {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::IndexActions => "Index actions",
            Self::SearchFiles => "Search files",
            Self::ParseTable => "Parse table data",
            Self::ProjectTable => "Build table view",
            Self::ExportTable => "Export table selection",
            Self::ReadTableFile => "Read table file",
            Self::FindDuplicates => "Find duplicate files",
            Self::CalculateHash => "Calculate file hash",
            Self::ScanRenameTargets => "Scan rename targets",
            Self::ApplyRename => "Apply rename plan",
            Self::UndoRename => "Undo rename transaction",
            Self::SearchFilesForReplacement => "Search replacement targets",
            Self::PreviewReplacement => "Build replacement preview",
            Self::ConfirmReplacement => "Confirm replacement preview",
            Self::ReplaceFiles => "Replace files",
            Self::FormatXml => "Format XML",
            Self::ParseXml => "Parse XML source",
            Self::InspectXmlNode => "Inspect XML node",
            Self::QueryXml => "Run XML query",
            Self::ValidateXmlSchema => "Validate XML schema",
            Self::LoadXmlFile => "Load XML file",
            Self::PrepareXmlSave => "Prepare XML save",
            Self::SaveXmlFile => "Save XML file",
            Self::CompareFolders => "Compare folders",
            Self::WatchHotFolder => "Process Hot Folder rules",
            Self::CaptureScreenshot => "Capture screenshot",
            Self::CaptureOcr => "Recognize selected text",
            Self::CaptureLivePin => "Capture live pin",
            Self::PlayMacro => "Play macro",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityOutcome {
    Completed,
    Cancelled,
    Failed,
}

impl ActivityOutcome {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityLifecycle {
    Queued,
    Running,
    Cancelling,
    Terminal(ActivityOutcome),
}

impl ActivityLifecycle {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::Cancelling => "Cancelling",
            Self::Terminal(outcome) => outcome.label(),
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Terminal(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityRetirement {
    Pending,
    Confirmed,
    Unavailable,
}

impl ActivityRetirement {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "Retirement pending",
            Self::Confirmed => "Retired",
            Self::Unavailable => "Retirement unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityProgressUnit {
    Items,
    Files,
    Rows,
    Bytes,
    Candidates,
    Steps,
    Phases,
}

impl ActivityProgressUnit {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Items => "items",
            Self::Files => "files",
            Self::Rows => "rows",
            Self::Bytes => "bytes",
            Self::Candidates => "candidates",
            Self::Steps => "steps",
            Self::Phases => "phases",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivityProgress {
    pub completed: u64,
    pub total: Option<u64>,
    pub unit: ActivityProgressUnit,
}

impl ActivityProgress {
    #[must_use]
    pub const fn new(completed: u64, total: Option<u64>, unit: ActivityProgressUnit) -> Self {
        Self {
            completed,
            total,
            unit,
        }
    }
}

/// Diagnostics are deliberately closed over fixed labels so paths, queries,
/// clipboard text and other payload-bearing strings cannot enter history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityDiagnostic {
    InvalidInput,
    WorkerUnavailable,
    WorkerFailed,
    NativeOperationUnavailable,
    Retiring,
    ResultUnavailable,
    CancelRequestFailed,
}

impl ActivityDiagnostic {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::InvalidInput => "Input was rejected",
            Self::WorkerUnavailable => "Worker unavailable",
            Self::WorkerFailed => "Worker failed",
            Self::NativeOperationUnavailable => "Native operation unavailable",
            Self::Retiring => "Worker is retiring",
            Self::ResultUnavailable => "Result unavailable",
            Self::CancelRequestFailed => "Cancel request failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActivityResourceFacts {
    /// Bytes charged by this owner according to its existing accounting.
    pub owner_accounted_bytes: Option<u64>,
    /// A non-authoritative estimate, kept separate from owner accounting.
    pub estimated_bytes: Option<u64>,
    /// Current process total; never attributed to an individual owner.
    pub process_working_set_bytes: Option<u64>,
    /// Current process total; never attributed to an individual owner.
    pub process_private_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ActivitySlotKey {
    owner: ActivityOwner,
    incarnation: ActivityIncarnation,
    slot: ActivitySlot,
}

#[derive(Clone)]
struct ActivityRecord {
    key: ActivityKey,
    slot: ActivitySlotKey,
    operation: ActivityOperation,
    lifecycle: ActivityLifecycle,
    retirement: ActivityRetirement,
    started: Option<Instant>,
    updated: Instant,
    progress: Option<ActivityProgress>,
    diagnostic: Option<ActivityDiagnostic>,
    resources: ActivityResourceFacts,
    cancel: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    finished_elapsed: Option<Duration>,
    retention: Arc<AtomicU8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityRow {
    pub key: ActivityKey,
    pub operation: ActivityOperation,
    pub lifecycle: ActivityLifecycle,
    pub retirement: ActivityRetirement,
    /// Unknown when observation attached after admission or the owner did not
    /// retain an authoritative start timestamp.
    pub elapsed: Option<Duration>,
    pub progress: Option<ActivityProgress>,
    pub diagnostic: Option<ActivityDiagnostic>,
    pub resources: ActivityResourceFacts,
    pub can_cancel: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivitySnapshot {
    pub rows: Vec<ActivityRow>,
    /// Number of active facts currently omitted by the global retention cap.
    pub omitted_active_facts: usize,
    /// Number of terminal facts omitted since the last clear operation.
    pub omitted_terminal_facts: usize,
    /// Finite owner summaries make each overflow count attributable without
    /// retaining an unbounded per-operation ledger.
    pub omissions_by_owner: Vec<ActivityOmissionSummary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivityOmissionSummary {
    pub owner: ActivityOwner,
    pub active: usize,
    pub terminal: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct ActivityOmissionCounts {
    active: usize,
    terminal: usize,
}

#[derive(Default)]
struct Registry {
    next_operation_id: u64,
    current: BTreeMap<ActivityKey, ActivityRecord>,
    latest_by_slot: BTreeMap<ActivitySlotKey, ActivityKey>,
    history: VecDeque<ActivityRecord>,
    omissions_by_owner: BTreeMap<ActivityOwner, ActivityOmissionCounts>,
}

#[derive(Clone, Default)]
pub struct ActivityCenter {
    registry: Arc<Mutex<Registry>>,
}

impl ActivityCenter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new immutable operation identity in one owner slot. The owner
    /// keeps control of admission; this registry only retains bounded facts.
    pub fn start(
        &self,
        owner: ActivityOwner,
        slot: ActivitySlot,
        incarnation: ActivityIncarnation,
        operation: ActivityOperation,
    ) -> Option<ActivityTask> {
        self.insert(
            owner,
            slot,
            incarnation,
            operation,
            Some(Instant::now()),
            ActivityLifecycle::Queued,
        )
    }

    /// Observe already-admitted queued work when the owner can only attach
    /// after admission. The elapsed time remains unknown instead of claiming
    /// the observation time as the operation's start.
    pub fn observe_queued(
        &self,
        owner: ActivityOwner,
        slot: ActivitySlot,
        incarnation: ActivityIncarnation,
        operation: ActivityOperation,
    ) -> Option<ActivityTask> {
        self.insert(
            owner,
            slot,
            incarnation,
            operation,
            None,
            ActivityLifecycle::Queued,
        )
    }

    /// Observe already-running work when the owner can only attach after the
    /// operation began. The elapsed time remains unknown instead of claiming
    /// the observation time as the operation's start.
    pub fn observe_running(
        &self,
        owner: ActivityOwner,
        slot: ActivitySlot,
        incarnation: ActivityIncarnation,
        operation: ActivityOperation,
    ) -> Option<ActivityTask> {
        self.insert(
            owner,
            slot,
            incarnation,
            operation,
            None,
            ActivityLifecycle::Running,
        )
    }

    fn insert(
        &self,
        owner: ActivityOwner,
        slot: ActivitySlot,
        incarnation: ActivityIncarnation,
        operation: ActivityOperation,
        started: Option<Instant>,
        lifecycle: ActivityLifecycle,
    ) -> Option<ActivityTask> {
        let now = Instant::now();
        let mut registry = lock_registry(&self.registry);
        let operation_id = registry.next_operation_id.checked_add(1)?;
        registry.next_operation_id = operation_id;
        let key = ActivityKey {
            owner,
            incarnation,
            operation_id: ActivityOperationId(operation_id),
        };
        let retention = Arc::new(AtomicU8::new(RETENTION_RECORDED));
        registry.current.insert(
            key,
            ActivityRecord {
                key,
                slot: ActivitySlotKey {
                    owner,
                    incarnation,
                    slot,
                },
                operation,
                lifecycle,
                retirement: ActivityRetirement::Pending,
                started,
                updated: now,
                progress: None,
                diagnostic: None,
                resources: ActivityResourceFacts::default(),
                cancel: None,
                finished_elapsed: None,
                retention: Arc::clone(&retention),
            },
        );
        registry.latest_by_slot.insert(
            ActivitySlotKey {
                owner,
                incarnation,
                slot,
            },
            key,
        );
        while registry.current.len() > ACTIVE_FACTS_LIMIT {
            let oldest = registry
                .current
                .keys()
                .min_by_key(|key| key.operation_id)
                .copied();
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(record) = registry.current.remove(&oldest) {
                record
                    .retention
                    .store(RETENTION_OMITTED_ACTIVE, Ordering::Release);
            }
            registry
                .latest_by_slot
                .retain(|_, current| *current != oldest);
            let counts = registry.omissions_by_owner.entry(oldest.owner).or_default();
            counts.active = counts.active.saturating_add(1);
        }
        Some(ActivityTask {
            center: self.clone(),
            slot: ActivitySlotKey {
                owner,
                incarnation,
                slot,
            },
            key,
            retention,
        })
    }

    #[must_use]
    pub fn snapshot(&self) -> ActivitySnapshot {
        let registry = lock_registry(&self.registry);
        let now = Instant::now();
        let mut records = registry
            .current
            .values()
            .chain(registry.history.iter())
            .map(|record| row_from_record(record, now))
            .collect::<Vec<_>>();
        records.sort_by_key(|row| row.key.operation_id);
        let omissions_by_owner = registry
            .omissions_by_owner
            .iter()
            .filter_map(|(owner, counts)| {
                (counts.active > 0 || counts.terminal > 0).then_some(ActivityOmissionSummary {
                    owner: *owner,
                    active: counts.active,
                    terminal: counts.terminal,
                })
            })
            .collect::<Vec<_>>();
        ActivitySnapshot {
            rows: records,
            omitted_active_facts: omissions_by_owner
                .iter()
                .fold(0_usize, |total, item| total.saturating_add(item.active)),
            omitted_terminal_facts: omissions_by_owner
                .iter()
                .fold(0_usize, |total, item| total.saturating_add(item.terminal)),
            omissions_by_owner,
        }
    }

    /// Clear completed rows. Active operations remain live and may publish a
    /// later terminal result; late retirement callbacks cannot recreate rows
    /// because retirement updates only mutate retained terminal records.
    pub fn clear_terminal_history(&self) {
        let mut registry = lock_registry(&self.registry);
        registry.history.clear();
        for counts in registry.omissions_by_owner.values_mut() {
            counts.terminal = 0;
        }
        registry
            .omissions_by_owner
            .retain(|_, counts| counts.active > 0);
    }

    #[must_use]
    pub fn terminal_history_len(&self) -> usize {
        lock_registry(&self.registry).history.len()
    }

    pub fn request_cancel(&self, key: ActivityKey) -> ActivityCancelResult {
        let (handler, previous_lifecycle) = {
            let mut registry = lock_registry(&self.registry);
            let Some(record) = registry.current.get_mut(&key) else {
                return if registry.history.iter().any(|record| record.key == key) {
                    ActivityCancelResult::Terminal
                } else {
                    ActivityCancelResult::Stale
                };
            };
            if record.lifecycle.is_terminal() {
                return ActivityCancelResult::Terminal;
            }
            let Some(handler) = record.cancel.clone() else {
                return ActivityCancelResult::Unsupported;
            };
            let previous_lifecycle = record.lifecycle;
            record.lifecycle = ActivityLifecycle::Cancelling;
            record.updated = Instant::now();
            (handler, previous_lifecycle)
        };
        if catch_unwind(AssertUnwindSafe(|| handler())).is_ok() {
            ActivityCancelResult::Requested
        } else {
            self.update_current(key, |record| {
                if record.lifecycle == ActivityLifecycle::Cancelling {
                    record.lifecycle = previous_lifecycle;
                }
                record.diagnostic = Some(ActivityDiagnostic::CancelRequestFailed);
            });
            ActivityCancelResult::HandlerFailed
        }
    }

    fn update_current(&self, key: ActivityKey, update: impl FnOnce(&mut ActivityRecord)) {
        let mut registry = lock_registry(&self.registry);
        if let Some(record) = registry.current.get_mut(&key) {
            update(record);
            record.updated = Instant::now();
        }
    }

    fn update_terminal(&self, key: ActivityKey, update: impl FnOnce(&mut ActivityRecord)) {
        let mut registry = lock_registry(&self.registry);
        if let Some(record) = registry.history.iter_mut().find(|record| record.key == key) {
            update(record);
            record.updated = Instant::now();
        } else if let Some(record) = registry.current.get_mut(&key) {
            update(record);
            record.updated = Instant::now();
        }
    }

    fn finish(
        &self,
        slot: ActivitySlotKey,
        key: ActivityKey,
        outcome: ActivityOutcome,
        diagnostic: Option<ActivityDiagnostic>,
        retirement: ActivityRetirement,
        retention: &AtomicU8,
    ) {
        let mut registry = lock_registry(&self.registry);
        let Some(mut record) = registry.current.remove(&key) else {
            if retention
                .compare_exchange(
                    RETENTION_OMITTED_ACTIVE,
                    RETENTION_OMITTED_TERMINAL,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                let counts = registry.omissions_by_owner.entry(key.owner).or_default();
                counts.active = counts.active.saturating_sub(1);
                counts.terminal = counts.terminal.saturating_add(1);
            }
            return;
        };
        if record.slot != slot {
            registry.current.insert(key, record);
            return;
        }
        let now = Instant::now();
        record.lifecycle = ActivityLifecycle::Terminal(outcome);
        record.retirement = retirement;
        record.diagnostic = diagnostic.or(record.diagnostic);
        record.cancel = None;
        record.updated = now;
        record.finished_elapsed = record
            .started
            .map(|started| now.saturating_duration_since(started));
        record
            .retention
            .store(RETENTION_TERMINAL_RETAINED, Ordering::Release);
        if registry.latest_by_slot.get(&slot) == Some(&key) {
            registry.latest_by_slot.remove(&slot);
        }
        registry.history.push_back(record);
        while registry.history.len() > TERMINAL_HISTORY_LIMIT {
            if let Some(omitted) = registry.history.pop_front() {
                omitted
                    .retention
                    .store(RETENTION_OMITTED_TERMINAL, Ordering::Release);
                let counts = registry
                    .omissions_by_owner
                    .entry(omitted.key.owner)
                    .or_default();
                counts.terminal = counts.terminal.saturating_add(1);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityCancelResult {
    Requested,
    Unsupported,
    Stale,
    Terminal,
    HandlerFailed,
}

#[derive(Clone)]
pub struct ActivityTask {
    center: ActivityCenter,
    slot: ActivitySlotKey,
    key: ActivityKey,
    retention: Arc<AtomicU8>,
}

impl ActivityTask {
    #[must_use]
    pub const fn key(&self) -> ActivityKey {
        self.key
    }

    pub fn mark_running(&self) {
        self.center.update_current(self.key, |record| {
            if record.lifecycle == ActivityLifecycle::Queued {
                record.lifecycle = ActivityLifecycle::Running;
            }
        });
    }

    pub fn mark_cancelling(&self) {
        self.center.update_current(self.key, |record| {
            if matches!(
                record.lifecycle,
                ActivityLifecycle::Queued | ActivityLifecycle::Running
            ) {
                record.lifecycle = ActivityLifecycle::Cancelling;
            }
        });
    }

    pub fn set_progress(&self, progress: Option<ActivityProgress>) {
        self.center
            .update_current(self.key, |record| record.progress = progress);
    }

    pub fn set_diagnostic(&self, diagnostic: Option<ActivityDiagnostic>) {
        self.center
            .update_current(self.key, |record| record.diagnostic = diagnostic);
    }

    pub fn set_resources(&self, resources: ActivityResourceFacts) {
        self.center
            .update_current(self.key, |record| record.resources = resources);
    }

    /// This is an explicit capability: callers must capture the exact owner
    /// operation's safe cancellation token or generation in the closure.
    pub fn set_cancel_handler(&self, handler: impl Fn() + Send + Sync + 'static) {
        self.center
            .update_current(self.key, |record| record.cancel = Some(Arc::new(handler)));
    }

    pub fn clear_cancel_handler(&self) {
        self.center
            .update_current(self.key, |record| record.cancel = None);
    }

    pub fn finish(
        &self,
        outcome: ActivityOutcome,
        diagnostic: Option<ActivityDiagnostic>,
        retirement: ActivityRetirement,
    ) {
        self.center.finish(
            self.slot,
            self.key,
            outcome,
            diagnostic,
            retirement,
            &self.retention,
        );
    }

    pub fn retirement_confirmed(&self) {
        self.center.update_terminal(self.key, |record| {
            record.retirement = ActivityRetirement::Confirmed
        });
    }

    pub fn retirement_unavailable(&self) {
        self.center.update_terminal(self.key, |record| {
            record.retirement = ActivityRetirement::Unavailable
        });
    }
}

fn row_from_record(record: &ActivityRecord, now: Instant) -> ActivityRow {
    ActivityRow {
        key: record.key,
        operation: record.operation,
        lifecycle: record.lifecycle,
        retirement: record.retirement,
        elapsed: if record.lifecycle.is_terminal() {
            record.finished_elapsed
        } else {
            record
                .started
                .map(|started| now.saturating_duration_since(started))
        },
        progress: record.progress,
        diagnostic: record.diagnostic,
        resources: record.resources,
        can_cancel: record.cancel.is_some() && !record.lifecycle.is_terminal(),
    }
}

fn lock_registry(registry: &Mutex<Registry>) -> MutexGuard<'_, Registry> {
    registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(center: &ActivityCenter, incarnation: u64) -> ActivityTask {
        center
            .start(
                ActivityOwner::FileSearch,
                ActivitySlot::Primary,
                ActivityIncarnation::new(incarnation),
                ActivityOperation::SearchFiles,
            )
            .expect("operation id remains available")
    }

    #[test]
    fn immediate_completion_is_retained_and_retirement_updates_the_same_row() {
        let center = ActivityCenter::new();
        let task = start(&center, 1);
        task.mark_running();
        task.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Pending,
        );
        task.retirement_confirmed();

        let snapshot = center.snapshot();
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].key, task.key());
        assert_eq!(
            snapshot.rows[0].lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Completed)
        );
        assert_eq!(snapshot.rows[0].retirement, ActivityRetirement::Confirmed);
    }

    #[test]
    fn replaced_slot_keeps_the_older_operation_until_its_terminal_update() {
        let center = ActivityCenter::new();
        let old = start(&center, 1);
        let latest = start(&center, 2);
        old.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Confirmed,
        );
        assert_eq!(
            center.request_cancel(old.key()),
            ActivityCancelResult::Terminal
        );
        let snapshot = center.snapshot();
        assert_eq!(snapshot.rows.len(), 2);
        assert!(snapshot.rows.iter().any(|row| row.key == latest.key()));
        assert!(snapshot.rows.iter().any(|row| {
            row.key == old.key()
                && row.lifecycle == ActivityLifecycle::Terminal(ActivityOutcome::Completed)
        }));
    }

    #[test]
    fn concurrent_instances_remain_visible_without_inventing_start_times() {
        let center = ActivityCenter::new();
        let first = center
            .observe_running(
                ActivityOwner::LivePin,
                ActivitySlot::LiveCapture,
                ActivityIncarnation::new(10),
                ActivityOperation::CaptureLivePin,
            )
            .expect("first observation");
        let second = center
            .observe_running(
                ActivityOwner::LivePin,
                ActivitySlot::LiveCapture,
                ActivityIncarnation::new(11),
                ActivityOperation::CaptureLivePin,
            )
            .expect("second observation");

        let snapshot = center.snapshot();
        assert_eq!(snapshot.rows.len(), 2);
        assert!(snapshot.rows.iter().all(|row| row.elapsed.is_none()));
        assert_ne!(first.key(), second.key());

        first.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Unavailable,
        );
        assert_eq!(center.snapshot().rows.len(), 2);
        second.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Unavailable,
        );
        assert!(
            center
                .snapshot()
                .rows
                .iter()
                .all(|row| row.elapsed.is_none())
        );
    }

    #[test]
    fn cancel_requires_the_exact_current_owner_capability() {
        let center = ActivityCenter::new();
        let task = start(&center, 1);
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_for_handler = Arc::clone(&calls);
        task.set_cancel_handler(move || {
            calls_for_handler.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        assert_eq!(
            center.request_cancel(task.key()),
            ActivityCancelResult::Requested
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            center.snapshot().rows[0].lifecycle,
            ActivityLifecycle::Cancelling
        );
        task.mark_running();
        assert_eq!(
            center.snapshot().rows[0].lifecycle,
            ActivityLifecycle::Cancelling
        );
        task.finish(
            ActivityOutcome::Cancelled,
            None,
            ActivityRetirement::Unavailable,
        );
        assert_eq!(
            center.request_cancel(task.key()),
            ActivityCancelResult::Terminal
        );
    }

    #[test]
    fn history_is_capped_and_clear_fences_late_retirement() {
        let center = ActivityCenter::new();
        let mut last = None;
        for incarnation in 0..(TERMINAL_HISTORY_LIMIT as u64 + 5) {
            let task = start(&center, incarnation);
            task.finish(
                ActivityOutcome::Completed,
                None,
                ActivityRetirement::Pending,
            );
            last = Some(task);
        }
        assert_eq!(center.terminal_history_len(), TERMINAL_HISTORY_LIMIT);
        let stale = last.expect("one task was started");
        center.clear_terminal_history();
        stale.retirement_confirmed();
        assert!(center.snapshot().rows.is_empty());
        assert_eq!(center.terminal_history_len(), 0);
    }

    #[test]
    fn repeated_superseded_incarnations_have_bounded_owner_overflow_summaries() {
        let center = ActivityCenter::new();
        let tasks = (0..(ACTIVE_FACTS_LIMIT + 44))
            .map(|incarnation| start(&center, incarnation as u64))
            .collect::<Vec<_>>();
        let active = center.snapshot();
        assert_eq!(active.rows.len(), ACTIVE_FACTS_LIMIT);
        assert_eq!(active.omitted_active_facts, 44);
        assert_eq!(active.omitted_terminal_facts, 0);
        assert_eq!(active.omissions_by_owner.len(), 1);
        assert_eq!(
            active.omissions_by_owner[0].owner,
            ActivityOwner::FileSearch
        );
        assert_eq!(active.omissions_by_owner[0].active, 44);

        for task in &tasks {
            task.finish(
                ActivityOutcome::Completed,
                None,
                ActivityRetirement::Unavailable,
            );
        }
        let terminal = center.snapshot();
        assert_eq!(terminal.rows.len(), TERMINAL_HISTORY_LIMIT);
        assert_eq!(terminal.omitted_active_facts, 0);
        assert_eq!(terminal.omitted_terminal_facts, 200);
        assert_eq!(terminal.omissions_by_owner.len(), 1);
        assert_eq!(terminal.omissions_by_owner[0].terminal, 200);

        center.clear_terminal_history();
        let cleared = center.snapshot();
        assert!(cleared.rows.is_empty());
        assert_eq!(cleared.omitted_terminal_facts, 0);
        assert!(cleared.omissions_by_owner.is_empty());
    }

    #[test]
    fn clear_preserves_a_live_operation_that_finishes_later() {
        let center = ActivityCenter::new();
        let task = start(&center, 1);
        center.clear_terminal_history();
        task.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Pending,
        );
        assert_eq!(center.terminal_history_len(), 1);
        task.retirement_confirmed();
        assert_eq!(
            center.snapshot().rows[0].retirement,
            ActivityRetirement::Confirmed
        );
    }

    #[test]
    fn terminal_elapsed_time_is_frozen_across_late_retirement() {
        let center = ActivityCenter::new();
        let task = start(&center, 1);
        task.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Pending,
        );
        let elapsed = center.snapshot().rows[0].elapsed;
        std::thread::sleep(Duration::from_millis(2));
        task.retirement_confirmed();
        assert_eq!(center.snapshot().rows[0].elapsed, elapsed);
    }

    #[test]
    fn late_attached_observation_keeps_unknown_terminal_elapsed() {
        let center = ActivityCenter::new();
        let queued = center
            .observe_queued(
                ActivityOwner::Ocr,
                ActivitySlot::Primary,
                ActivityIncarnation::new(3),
                ActivityOperation::CaptureOcr,
            )
            .expect("late queued observation");
        let running = center
            .observe_running(
                ActivityOwner::Ocr,
                ActivitySlot::Primary,
                ActivityIncarnation::new(4),
                ActivityOperation::CaptureOcr,
            )
            .expect("late running observation");
        queued.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Unavailable,
        );
        running.finish(
            ActivityOutcome::Completed,
            None,
            ActivityRetirement::Unavailable,
        );
        std::thread::sleep(Duration::from_millis(2));
        let snapshot = center.snapshot();
        for incarnation in [3, 4] {
            let row = snapshot
                .rows
                .iter()
                .find(|row| row.key.incarnation().get() == incarnation)
                .expect("late observation remains retained");
            assert_eq!(row.elapsed, None);
        }
    }

    #[test]
    fn diagnostics_are_fixed_labels_and_progress_is_coalesced() {
        let center = ActivityCenter::new();
        let task = start(&center, 1);
        task.set_diagnostic(Some(ActivityDiagnostic::WorkerFailed));
        task.set_progress(Some(ActivityProgress::new(
            3,
            Some(10),
            ActivityProgressUnit::Files,
        )));
        task.set_progress(Some(ActivityProgress::new(
            8,
            Some(10),
            ActivityProgressUnit::Files,
        )));
        let row = &center.snapshot().rows[0];
        assert_eq!(row.progress.map(|progress| progress.completed), Some(8));
        assert_eq!(
            row.diagnostic.map(ActivityDiagnostic::label),
            Some("Worker failed")
        );
    }
}
