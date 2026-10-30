//! Ephemeral resources used while a retained folder comparison is alive.
//!
//! This type deliberately lives outside `FolderCompareState`: retained views
//! may be cloned or persisted, while receivers and operation handles may not.

use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityIncarnation, ActivityOperation, ActivityOutcome,
    ActivityOwner, ActivityRetirement, ActivitySlot, ActivityTask,
};
use crate::diff::file_ops::{ExecutionReport, ItemOutcome, OperationEvent, OperationHandle};
use crate::diff::folder_compare::{EntryMetadata, FolderEntry, FolderModel, FolderStatus};
use crate::diff::folder_scan::{RootIdentity, ScanHandle};
use crate::diff::text_compare::{CompiledRules, TextComparisonRules, project};
use crate::diff::text_file::{LoadedContent, load_text_file};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderComparisonIdentity {
    pub view_id: u64,
    pub left_root: RootIdentity,
    pub right_root: RootIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderOperationContext {
    pub comparison: FolderComparisonIdentity,
    pub generation: u64,
}

struct ActiveFolderOperation {
    context: FolderOperationContext,
    handle: OperationHandle,
}

pub(crate) const MAX_OPERATION_FAILURE_DETAILS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderOperationFailure {
    pub relative: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderOperationFeedback {
    pub completed: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub failures: Vec<FolderOperationFailure>,
    pub omitted_failures: usize,
}

impl FolderOperationFeedback {
    fn from_report(report: &ExecutionReport) -> Self {
        let mut feedback = Self {
            completed: 0,
            failed: 0,
            cancelled: report.cancelled,
            failures: Vec::new(),
            omitted_failures: 0,
        };
        for item in &report.items {
            match &item.outcome {
                ItemOutcome::Copied | ItemOutcome::Overwritten | ItemOutcome::CreatedDirectory => {
                    feedback.completed += 1;
                }
                ItemOutcome::Failed(message) => {
                    feedback.failed += 1;
                    if feedback.failures.len() < MAX_OPERATION_FAILURE_DETAILS {
                        feedback.failures.push(FolderOperationFailure {
                            relative: item.relative.clone(),
                            message: message.clone(),
                        });
                    } else {
                        feedback.omitted_failures += 1;
                    }
                }
                ItemOutcome::Cancelled => feedback.cancelled = true,
            }
        }
        feedback
    }
}

pub struct FolderRuntime {
    pub left_scan: Option<ScanHandle>,
    pub right_scan: Option<ScanHandle>,
    pub generation: u64,
    pub left_root: Option<RootIdentity>,
    pub right_root: Option<RootIdentity>,
    pub left_visited: u64,
    pub right_visited: u64,
    pub comparison_queue: VecDeque<PathBuf>,
    queued: HashSet<PathBuf>,
    in_flight: HashSet<PathBuf>,
    result_tx: Sender<RefinementResult>,
    result_rx: Receiver<RefinementResult>,
    pub completed_comparisons: u64,
    active_operation: Option<ActiveFolderOperation>,
    // Scan identities are cleared on rescan; this identity owns retained feedback.
    comparison: Option<FolderComparisonIdentity>,
    pub(crate) operation_feedback: Option<FolderOperationFeedback>,
    activity_center: Option<ActivityCenter>,
    refinement_slots: HashMap<PathBuf, (u64, ActivitySlot)>,
    pub left_error: Option<String>,
    pub right_error: Option<String>,
    pub restart_prepared: bool,
    // Filesystem watchers belong here when live refresh is implemented.
}

const MAX_IN_FLIGHT: usize = 2;
const MAX_CONTENT_BYTES: u64 = 32 * 1024 * 1024;
const BLOCK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct RefinementJob {
    pub left_root: RootIdentity,
    pub right_root: RootIdentity,
    pub relative_path: PathBuf,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub left_identity: EntryMetadata,
    pub right_identity: EntryMetadata,
    pub generation: u64,
    pub rules: TextComparisonRules,
    pub rules_revision: u64,
}

#[derive(Debug)]
struct RefinementResult {
    job: RefinementJob,
    status: FolderStatus,
}

impl Default for FolderRuntime {
    fn default() -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        Self {
            left_scan: None,
            right_scan: None,
            generation: 0,
            left_root: None,
            right_root: None,
            left_visited: 0,
            right_visited: 0,
            comparison_queue: VecDeque::new(),
            queued: HashSet::new(),
            in_flight: HashSet::new(),
            result_tx,
            result_rx,
            completed_comparisons: 0,
            active_operation: None,
            comparison: None,
            operation_feedback: None,
            activity_center: None,
            refinement_slots: HashMap::new(),
            left_error: None,
            right_error: None,
            restart_prepared: false,
        }
    }
}

impl FolderRuntime {
    pub fn attach_activity_center(&mut self, center: ActivityCenter) {
        self.activity_center = Some(center);
    }

    pub fn spawn_scan(
        &self,
        root: PathBuf,
        generation: u64,
        rules: crate::diff::folder_scan::ScanRules,
        slot: ActivitySlot,
    ) -> ScanHandle {
        if let Some(activity_center) = self.activity_center.as_ref() {
            crate::diff::folder_scan::spawn_scan_with_activity(
                Arc::new(crate::diff::folder_scan::RealFileSystem),
                root,
                generation,
                rules,
                128,
                activity_center.clone(),
                slot,
            )
        } else {
            crate::diff::folder_scan::spawn_scan(root, generation, rules)
        }
    }

    pub fn next_generation() -> u64 {
        Self::next_generation_after(0)
    }

    fn next_generation_after(current: u64) -> u64 {
        let mut observed = NEXT_GENERATION.load(Ordering::Relaxed);
        loop {
            let next = observed
                .max(current)
                .checked_add(1)
                .expect("folder scan generation exhausted");
            match NEXT_GENERATION.compare_exchange_weak(
                observed,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return next,
                Err(actual) => observed = actual,
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.left_scan.is_some()
            || self.right_scan.is_some()
            || !self.comparison_queue.is_empty()
            || !self.in_flight.is_empty()
            || self.mutation_active()
    }

    /// Mutation plans live in dialog state; dispatch ownership and delivered
    /// feedback remain with the retained comparison's ephemeral runtime.
    pub fn mutation_active(&self) -> bool {
        self.active_operation.is_some()
    }

    pub(crate) fn belongs_to(&self, comparison: &FolderComparisonIdentity) -> bool {
        self.comparison
            .as_ref()
            .is_none_or(|current| current == comparison)
    }

    pub(crate) fn reconcile_comparison(&mut self, comparison: FolderComparisonIdentity) {
        if !self.belongs_to(&comparison) {
            self.prepare_rescan();
            self.active_operation = None;
            self.operation_feedback = None;
        }
        self.comparison = Some(comparison);
    }

    pub(crate) fn start_operation(
        &mut self,
        context: FolderOperationContext,
        handle: OperationHandle,
    ) {
        self.comparison = Some(context.comparison.clone());
        self.operation_feedback = None;
        self.active_operation = Some(ActiveFolderOperation { context, handle });
    }

    pub(crate) fn operation_events(&self) -> Option<(FolderOperationContext, Vec<OperationEvent>)> {
        self.active_operation.as_ref().map(|operation| {
            (
                operation.context.clone(),
                operation.handle.receiver.try_iter().collect(),
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn wait_for_operation_event(&self) -> OperationEvent {
        self.active_operation
            .as_ref()
            .expect("test dispatched an operation")
            .handle
            .receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("task-owned operation must finish before fixture cleanup")
    }

    /// A terminal acknowledgement releases only its own dispatch. A rescan may
    /// have advanced the generation while cancellation was still pending.
    /// Return true only when current-generation feedback was adopted.
    pub(crate) fn finish_operation(
        &mut self,
        context: &FolderOperationContext,
        report: &ExecutionReport,
    ) -> bool {
        if report.generation != context.generation
            || !self
                .active_operation
                .as_ref()
                .is_some_and(|operation| operation.context == *context)
        {
            return false;
        }
        self.active_operation = None;
        if context.generation != self.generation {
            return false;
        }
        self.operation_feedback = Some(FolderOperationFeedback::from_report(report));
        true
    }

    /// Rebuild the deduplicated queue in user-visible priority order.
    pub fn prioritize(
        &mut self,
        selected: Option<&PathBuf>,
        visible: &[PathBuf],
        model: &FolderModel,
    ) {
        let mut order = Vec::new();
        if let Some(path) = selected {
            order.push(path.clone());
        }
        order.extend(visible.iter().cloned());
        order.extend(model.entries.values().map(|e| e.relative_path.clone()));
        let mut queue = VecDeque::new();
        let mut queued = HashSet::new();
        for path in order {
            let pending = model.entries.values().any(|e| {
                e.relative_path == path
                    && e.effective_status == FolderStatus::PendingContentComparison
            });
            if pending && !self.in_flight.contains(&path) && queued.insert(path.clone()) {
                queue.push_back(path);
            }
        }
        self.comparison_queue = queue;
        self.queued = queued;
    }

    pub fn pump(&mut self, model: &mut FolderModel, rules: &TextComparisonRules) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.in_flight.remove(&result.job.relative_path);
            if self
                .refinement_slots
                .get(&result.job.relative_path)
                .is_some_and(|(generation, _)| *generation == result.job.generation)
            {
                self.refinement_slots.remove(&result.job.relative_path);
            }
            if self.result_is_current(&result.job, model, rules)
                && let Some(entry) = find_entry_mut(model, &result.job.relative_path)
            {
                entry.content_checked = true;
                entry.effective_status = result.status;
                self.completed_comparisons += 1;
                model.revision = model.revision.wrapping_add(1);
            }
        }
        while self.in_flight.len() < MAX_IN_FLIGHT {
            let Some(path) = self.comparison_queue.pop_front() else {
                break;
            };
            self.queued.remove(&path);
            let Some(entry) = model.entries.values().find(|e| e.relative_path == path) else {
                continue;
            };
            let Some(job) = self.make_job(entry, rules) else {
                continue;
            };
            let slot = if self
                .refinement_slots
                .values()
                .any(|(_, slot)| *slot == ActivitySlot::Primary)
            {
                ActivitySlot::Secondary
            } else {
                ActivitySlot::Primary
            };
            self.in_flight.insert(path.clone());
            self.refinement_slots.insert(path, (job.generation, slot));
            let activity = self.activity_center.as_ref().and_then(|center| {
                center.start(
                    ActivityOwner::FolderCompare,
                    slot,
                    ActivityIncarnation::new(job.generation),
                    ActivityOperation::CompareFolders,
                )
            });
            let tx = self.result_tx.clone();
            std::thread::spawn(move || {
                let mut activity = activity.map(FolderRefinementActivity::new);
                let status = compare_job(&job);
                if let Some(activity) = activity.take() {
                    activity.finish(status);
                }
                let _ = tx.send(RefinementResult { job, status });
            });
        }
    }

    fn make_job(&self, e: &FolderEntry, rules: &TextComparisonRules) -> Option<RefinementJob> {
        let (left, right) = (e.left.as_ref()?, e.right.as_ref()?);
        Some(RefinementJob {
            left_root: self.left_root.clone()?,
            right_root: self.right_root.clone()?,
            relative_path: e.relative_path.clone(),
            left_path: left.path.clone(),
            right_path: right.path.clone(),
            left_identity: left.metadata.clone()?,
            right_identity: right.metadata.clone()?,
            generation: self.generation,
            rules: rules.clone(),
            rules_revision: rules.revision,
        })
    }

    fn result_is_current(
        &self,
        job: &RefinementJob,
        model: &FolderModel,
        rules: &TextComparisonRules,
    ) -> bool {
        self.generation == job.generation
            && self.left_root.as_ref() == Some(&job.left_root)
            && self.right_root.as_ref() == Some(&job.right_root)
            && rules.revision == job.rules_revision
            && model
                .entries
                .values()
                .find(|e| e.relative_path == job.relative_path)
                .is_some_and(|e| {
                    e.effective_status == FolderStatus::PendingContentComparison
                        && e.left.as_ref().and_then(|s| s.metadata.as_ref())
                            == Some(&job.left_identity)
                        && e.right.as_ref().and_then(|s| s.metadata.as_ref())
                            == Some(&job.right_identity)
                })
    }
    pub fn cancel(&self) {
        if let Some(scan) = &self.left_scan {
            scan.cancel();
        }
        if let Some(scan) = &self.right_scan {
            scan.cancel();
        }
        if let Some(operation) = &self.active_operation {
            operation.handle.cancel();
        }
    }

    /// Cancel every class of work and establish a new generation before a
    /// replacement scan can emit events. Keep the mutation gate until its
    /// terminal acknowledgement: cancellation alone does not finish an effect.
    pub fn prepare_rescan(&mut self) {
        self.cancel();
        self.generation = Self::next_generation_after(self.generation);
        self.restart_prepared = true;
        self.left_root = None;
        self.right_root = None;
        self.left_scan = None;
        self.right_scan = None;
        self.comparison_queue.clear();
        self.queued.clear();
        self.in_flight.clear();
        self.refinement_slots.clear();
        self.left_visited = 0;
        self.right_visited = 0;
        self.completed_comparisons = 0;
        self.left_error = None;
        self.right_error = None;
    }
}

struct FolderRefinementActivity(Option<ActivityTask>);

impl FolderRefinementActivity {
    fn new(task: ActivityTask) -> Self {
        task.mark_running();
        Self(Some(task))
    }

    fn finish(mut self, status: FolderStatus) {
        if let Some(task) = self.0.take() {
            let failed = matches!(status, FolderStatus::Error | FolderStatus::Unreadable);
            task.finish(
                if failed {
                    ActivityOutcome::Failed
                } else {
                    ActivityOutcome::Completed
                },
                failed.then_some(ActivityDiagnostic::WorkerFailed),
                ActivityRetirement::Unavailable,
            );
        }
    }
}

impl Drop for FolderRefinementActivity {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.finish(
                ActivityOutcome::Failed,
                Some(ActivityDiagnostic::WorkerFailed),
                ActivityRetirement::Unavailable,
            );
        }
    }
}

fn find_entry_mut<'a>(model: &'a mut FolderModel, path: &PathBuf) -> Option<&'a mut FolderEntry> {
    model
        .entries
        .values_mut()
        .find(|e| &e.relative_path == path)
}

fn compare_job(job: &RefinementJob) -> FolderStatus {
    if job.left_identity.size > MAX_CONTENT_BYTES || job.right_identity.size > MAX_CONTENT_BYTES {
        return FolderStatus::Error;
    }
    match (
        load_text_file(&job.left_path),
        load_text_file(&job.right_path),
    ) {
        (Ok(a), Ok(b)) => match (&a.content, &b.content) {
            (LoadedContent::Text(a), LoadedContent::Text(b)) => {
                match CompiledRules::compile(&job.rules) {
                    Ok(c) => {
                        if project(a, &c)
                            .lines
                            .iter()
                            .map(|l| &l.significance_key)
                            .eq(project(b, &c).lines.iter().map(|l| &l.significance_key))
                        {
                            FolderStatus::Identical
                        } else {
                            FolderStatus::Different
                        }
                    }
                    Err(_) => FolderStatus::Error,
                }
            }
            (LoadedContent::Binary { .. }, LoadedContent::Binary { .. }) => match binary_equal(
                &job.left_path,
                &job.right_path,
                job.left_identity.size,
                job.right_identity.size,
            ) {
                Ok(true) => FolderStatus::Identical,
                Ok(false) => FolderStatus::Different,
                Err(e) => io_status(&e),
            },
            _ => FolderStatus::Different,
        },
        (Err(e), _) | (_, Err(e)) => io_status(&e),
    }
}

fn binary_equal(
    left: &PathBuf,
    right: &PathBuf,
    left_size: u64,
    right_size: u64,
) -> io::Result<bool> {
    if left_size != right_size {
        return Ok(false);
    }
    let (mut left, mut right) = (std::fs::File::open(left)?, std::fs::File::open(right)?);
    let (mut a, mut b) = (vec![0; BLOCK_BYTES], vec![0; BLOCK_BYTES]);
    loop {
        let an = left.read(&mut a)?;
        let bn = right.read(&mut b)?;
        if an != bn || a[..an] != b[..bn] {
            return Ok(false);
        }
        if an == 0 {
            return Ok(true);
        }
    }
}

fn io_status(error: &io::Error) -> FolderStatus {
    match error.kind() {
        io::ErrorKind::NotFound
        | io::ErrorKind::PermissionDenied
        | io::ErrorKind::UnexpectedEof => FolderStatus::Unreadable,
        _ => FolderStatus::Error,
    }
}

impl Drop for FolderRuntime {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::folder_compare::{EntryKind, EntrySide};
    use std::time::SystemTime;

    fn metadata(path: &std::path::Path) -> EntryMetadata {
        EntryMetadata::from_fs(&std::fs::metadata(path).unwrap(), EntryKind::File)
    }
    fn entry(relative: &str, left: PathBuf, right: PathBuf) -> FolderEntry {
        FolderEntry {
            relative_path: relative.into(),
            left: Some(EntrySide {
                metadata: Some(metadata(&left)),
                path: left,
                error: None,
            }),
            right: Some(EntrySide {
                metadata: Some(metadata(&right)),
                path: right,
                error: None,
            }),
            metadata_status: FolderStatus::PendingContentComparison,
            effective_status: FolderStatus::PendingContentComparison,
            content_checked: false,
        }
    }
    fn job(left: PathBuf, right: PathBuf, rules: TextComparisonRules) -> RefinementJob {
        RefinementJob {
            left_root: RootIdentity::new(left.parent().unwrap().into()),
            right_root: RootIdentity::new(right.parent().unwrap().into()),
            relative_path: "file".into(),
            left_identity: metadata(&left),
            right_identity: metadata(&right),
            left_path: left,
            right_path: right,
            generation: 1,
            rules_revision: rules.revision,
            rules,
        }
    }

    #[test]
    fn text_and_binary_equality_and_read_errors_are_statuses() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::write(&a, "Hello  world\n").unwrap();
        std::fs::write(&b, "hello world\n").unwrap();
        let rules = TextComparisonRules {
            ignore_all_whitespace: true,
            case_sensitive: false,
            revision: 3,
            ..Default::default()
        };
        assert_eq!(
            compare_job(&job(a.clone(), b.clone(), rules)),
            FolderStatus::Identical
        );
        std::fs::write(&b, "different").unwrap();
        assert_eq!(
            compare_job(&job(a.clone(), b.clone(), Default::default())),
            FolderStatus::Different
        );
        std::fs::write(&a, [0xff, 0, 1, 2]).unwrap();
        std::fs::write(&b, [0xff, 0, 1, 2]).unwrap();
        assert_eq!(
            compare_job(&job(a.clone(), b.clone(), Default::default())),
            FolderStatus::Identical
        );
        std::fs::write(&b, [0xff, 0, 1, 3]).unwrap();
        assert_eq!(
            compare_job(&job(a.clone(), b.clone(), Default::default())),
            FolderStatus::Different
        );
        let missing = dir.path().join("missing");
        let mut bad = job(a, b, Default::default());
        bad.left_path = missing;
        assert_eq!(compare_job(&bad), FolderStatus::Unreadable);
    }

    #[test]
    fn priority_queue_is_selected_visible_background_and_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = FolderModel::default();
        for name in ["background", "visible", "selected"] {
            let l = dir.path().join(format!("{name}-l"));
            let r = dir.path().join(format!("{name}-r"));
            std::fs::write(&l, name).unwrap();
            std::fs::write(&r, name).unwrap();
            model.entries.insert(name.into(), entry(name, l, r));
        }
        let mut runtime = FolderRuntime::default();
        let selected = PathBuf::from("selected");
        runtime.prioritize(Some(&selected), &["visible".into()], &model);
        runtime.prioritize(Some(&selected), &["visible".into()], &model);
        let queued: Vec<_> = runtime.comparison_queue.iter().cloned().collect();
        assert_eq!(
            queued,
            vec![
                PathBuf::from("selected"),
                PathBuf::from("visible"),
                PathBuf::from("background")
            ]
        );
    }

    #[test]
    fn prepare_rescan_cancels_work_clears_progress_and_increments_generation() {
        let mut runtime = FolderRuntime::default();
        runtime.generation = 10;
        runtime.left_visited = 5;
        runtime.right_visited = 6;
        runtime.comparison_queue.push_back("queued".into());
        runtime.in_flight.insert("running".into());
        runtime.completed_comparisons = 2;
        runtime.prepare_rescan();
        assert!(runtime.generation > 10);
        assert!(runtime.restart_prepared);
        assert_eq!((runtime.left_visited, runtime.right_visited), (0, 0));
        assert!(runtime.comparison_queue.is_empty() && runtime.in_flight.is_empty());
        assert_eq!(runtime.completed_comparisons, 0);
    }

    #[test]
    fn stale_generation_identity_and_rules_are_rejected_and_completion_updates_one_entry() {
        let dir = tempfile::tempdir().unwrap();
        let (l, r) = (dir.path().join("l"), dir.path().join("r"));
        std::fs::write(&l, "same").unwrap();
        std::fs::write(&r, "same").unwrap();
        let mut model = FolderModel::default();
        model.entries.insert("file".into(), entry("file", l, r));
        let mut runtime = FolderRuntime::default();
        runtime.generation = 9;
        runtime.left_root = Some(RootIdentity::new(dir.path().into()));
        runtime.right_root = runtime.left_root.clone();
        let rules = TextComparisonRules {
            revision: 4,
            ..Default::default()
        };
        let j = runtime
            .make_job(model.entries.get("file").unwrap(), &rules)
            .unwrap();
        assert!(runtime.result_is_current(&j, &model, &rules));
        let mut stale = j.clone();
        stale.generation -= 1;
        assert!(!runtime.result_is_current(&stale, &model, &rules));
        stale = j.clone();
        stale.left_identity.modified = Some(SystemTime::UNIX_EPOCH);
        assert!(!runtime.result_is_current(&stale, &model, &rules));
        let changed_rules = TextComparisonRules {
            revision: 5,
            ..Default::default()
        };
        assert!(!runtime.result_is_current(&j, &model, &changed_rules));
        runtime
            .result_tx
            .send(RefinementResult {
                job: j,
                status: FolderStatus::Identical,
            })
            .unwrap();
        runtime.pump(&mut model, &rules);
        let e = model.entries.get("file").unwrap();
        assert!(e.content_checked);
        assert_eq!(e.effective_status, FolderStatus::Identical);
    }
}
