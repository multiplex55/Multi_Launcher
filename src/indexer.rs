use crate::actions::Action;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use walkdir::{IntoIter as WalkDirIter, WalkDir};

pub mod coordinator;

#[cfg(test)]
std::thread_local! {
    static INDEX_BATCH_FACTORY_ENTRIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

const DEFAULT_BATCH_SIZE: usize = 512;
const DEFAULT_MAX_ITEMS: usize = 100_000;

#[derive(Debug, Clone, Copy)]
pub struct IndexOptions {
    pub batch_size: usize,
    pub max_items: usize,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            max_items: DEFAULT_MAX_ITEMS,
        }
    }
}

impl IndexOptions {
    pub fn with_max_items(max_items: Option<usize>) -> Self {
        Self {
            max_items: max_items.unwrap_or(DEFAULT_MAX_ITEMS),
            ..Self::default()
        }
    }
}

/// Lazily indexes files from one or more roots and yields actions in batches.
///
/// Duplicate files are skipped by canonical path. Traversal errors stop
/// iteration and are returned to the caller.
pub struct IndexBatchIter {
    roots: Vec<String>,
    root_idx: usize,
    current: Option<WalkDirIter>,
    seen: HashSet<PathBuf>,
    options: IndexOptions,
    produced: usize,
    cancellation: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    checkpoint_hook: Option<Arc<dyn Fn(IndexCheckpoint) + Send + Sync>>,
    metric_scan_started: bool,
    metric_scan_failed: bool,
    metric_scan_finished: bool,
}

impl IndexBatchIter {
    fn new(paths: &[String], options: IndexOptions) -> Self {
        Self::new_cancellable(paths, options, None)
    }

    fn new_cancellable(
        paths: &[String],
        options: IndexOptions,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> Self {
        let options = IndexOptions {
            batch_size: options.batch_size.max(1),
            max_items: options.max_items.max(1),
        };
        Self {
            roots: paths.to_vec(),
            root_idx: 0,
            current: None,
            seen: HashSet::new(),
            options,
            produced: 0,
            cancellation,
            #[cfg(test)]
            checkpoint_hook: None,
            metric_scan_started: false,
            metric_scan_failed: false,
            metric_scan_finished: false,
        }
    }

    fn next_root(&mut self) -> Option<String> {
        let root = self.roots.get(self.root_idx).cloned();
        if root.is_some() {
            self.root_idx += 1;
        }
        root
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Acquire))
    }

    #[cfg(test)]
    fn checkpoint(&self, checkpoint: IndexCheckpoint) {
        if let Some(hook) = self.checkpoint_hook.as_ref() {
            hook(checkpoint);
        }
    }

    #[cfg(test)]
    fn with_checkpoint_hook(mut self, hook: Arc<dyn Fn(IndexCheckpoint) + Send + Sync>) -> Self {
        self.checkpoint_hook = Some(hook);
        self
    }

    fn next_step(&mut self) -> anyhow::Result<IndexBatchStep> {
        // Each sample covers this traversal call only, not time between batches.
        let mut timer =
            crate::performance::MetricTimer::start(crate::performance::Metric::IndexScan);
        timer.set_work_units(0);
        if timer.is_enabled() {
            self.metric_scan_started = true;
        }

        if self.is_cancelled() {
            return Ok(IndexBatchStep::Cancelled);
        }
        if self.produced >= self.options.max_items {
            if self.is_cancelled() {
                return Ok(IndexBatchStep::Cancelled);
            }
            self.finish_metric_scan();
            return Ok(IndexBatchStep::Complete);
        }

        let mut batch = Vec::with_capacity(self.options.batch_size);
        while self.produced < self.options.max_items && batch.len() < self.options.batch_size {
            if self.is_cancelled() {
                return Ok(IndexBatchStep::Cancelled);
            }
            if self.current.is_none() {
                if self.is_cancelled() {
                    return Ok(IndexBatchStep::Cancelled);
                }
                if let Some(root) = self.next_root() {
                    if self.is_cancelled() {
                        return Ok(IndexBatchStep::Cancelled);
                    }
                    self.current = Some(WalkDir::new(root).into_iter());
                } else {
                    break;
                }
            }

            if self.is_cancelled() {
                return Ok(IndexBatchStep::Cancelled);
            }
            #[cfg(test)]
            self.checkpoint(IndexCheckpoint::BeforeEntry);
            if self.is_cancelled() {
                return Ok(IndexBatchStep::Cancelled);
            }

            let Some(iter) = self.current.as_mut() else {
                continue;
            };
            let next_entry = iter.next();

            match next_entry {
                Some(Ok(entry)) => {
                    if self.is_cancelled() {
                        return Ok(IndexBatchStep::Cancelled);
                    }
                    if !entry.file_type().is_file() {
                        #[cfg(test)]
                        self.checkpoint(IndexCheckpoint::SkippedEntry);
                        if self.is_cancelled() {
                            return Ok(IndexBatchStep::Cancelled);
                        }
                        continue;
                    }
                    if self.is_cancelled() {
                        return Ok(IndexBatchStep::Cancelled);
                    }
                    #[cfg(test)]
                    self.checkpoint(IndexCheckpoint::BeforeCanonicalize);
                    if self.is_cancelled() {
                        return Ok(IndexBatchStep::Cancelled);
                    }
                    let canonical = match fs::canonicalize(entry.path()) {
                        Ok(path) => path,
                        Err(err) => {
                            timer.set_work_units(batch.len() as u64);
                            self.fail_metric_scan();
                            tracing::error!(
                                path = %entry.path().display(),
                                error = %err,
                                "failed to canonicalize indexed path"
                            );
                            return Err(err.into());
                        }
                    };
                    #[cfg(test)]
                    self.checkpoint(IndexCheckpoint::AfterCanonicalize);
                    if self.is_cancelled() {
                        return Ok(IndexBatchStep::Cancelled);
                    }
                    if !self.seen.insert(canonical.clone()) {
                        #[cfg(test)]
                        self.checkpoint(IndexCheckpoint::SkippedEntry);
                        if self.is_cancelled() {
                            return Ok(IndexBatchStep::Cancelled);
                        }
                        continue;
                    }
                    let Some(name) = canonical.file_name().and_then(|n| n.to_str()) else {
                        #[cfg(test)]
                        self.checkpoint(IndexCheckpoint::SkippedEntry);
                        if self.is_cancelled() {
                            return Ok(IndexBatchStep::Cancelled);
                        }
                        continue;
                    };
                    if self.is_cancelled() {
                        return Ok(IndexBatchStep::Cancelled);
                    }
                    let display = canonical.display().to_string();
                    batch.push(Action {
                        label: name.to_string(),
                        desc: display.clone(),
                        action: display,
                        args: None,
                    });
                    self.produced += 1;
                }
                Some(Err(err)) => {
                    timer.set_work_units(batch.len() as u64);
                    self.fail_metric_scan();
                    tracing::error!(error = %err, "failed to read directory entry");
                    return Err(err.into());
                }
                None => {
                    self.current = None;
                }
            }
        }

        if self.is_cancelled() {
            return Ok(IndexBatchStep::Cancelled);
        }
        timer.set_work_units(batch.len() as u64);
        if batch.is_empty() {
            if self.is_cancelled() {
                return Ok(IndexBatchStep::Cancelled);
            }
            self.finish_metric_scan();
            Ok(IndexBatchStep::Complete)
        } else {
            if self.is_cancelled() {
                return Ok(IndexBatchStep::Cancelled);
            }
            // The legacy iterator considers its final yielded batch complete.
            // A coordinator scan must wait for the next terminal step so a
            // cancellation between that batch and exhaustion is not reported
            // as a completed scan.
            if self.produced >= self.options.max_items && self.cancellation.is_none() {
                self.finish_metric_scan();
            }
            Ok(IndexBatchStep::Batch(batch))
        }
    }
}

enum IndexBatchStep {
    Batch(Vec<Action>),
    Complete,
    Cancelled,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexCheckpoint {
    BeforeEntry,
    SkippedEntry,
    BeforeCanonicalize,
    AfterCanonicalize,
}

impl Iterator for IndexBatchIter {
    type Item = anyhow::Result<Vec<Action>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_step() {
            Ok(IndexBatchStep::Batch(batch)) => Some(Ok(batch)),
            Ok(IndexBatchStep::Complete | IndexBatchStep::Cancelled) => None,
            Err(error) => Some(Err(error)),
        }
    }
}

impl IndexBatchIter {
    fn finish_metric_scan(&mut self) {
        if self.metric_scan_finished {
            return;
        }
        if self.metric_scan_started && !self.metric_scan_failed {
            crate::performance::record_metric_outcome(
                crate::performance::Metric::IndexScan,
                crate::performance::MetricOutcome::Completed,
            );
        }
        self.metric_scan_finished = true;
    }

    fn fail_metric_scan(&mut self) {
        // The first error is terminal for metrics, but the iterator keeps its
        // existing behavior if a caller asks for later batches.
        if self.metric_scan_started && !self.metric_scan_failed {
            crate::performance::record_metric_outcome(
                crate::performance::Metric::IndexScan,
                crate::performance::MetricOutcome::Error,
            );
            self.metric_scan_failed = true;
        }
    }
}

impl Drop for IndexBatchIter {
    fn drop(&mut self) {
        if self.metric_scan_started && !self.metric_scan_failed && !self.metric_scan_finished {
            crate::performance::record_metric_outcome(
                crate::performance::Metric::IndexScan,
                crate::performance::MetricOutcome::Abandoned,
            );
        }
    }
}

pub fn index_paths_batched(paths: &[String], options: IndexOptions) -> IndexBatchIter {
    #[cfg(test)]
    INDEX_BATCH_FACTORY_ENTRIES.with(|entries| entries.set(entries.get().saturating_add(1)));
    IndexBatchIter::new(paths, options)
}

#[cfg(test)]
pub(crate) fn reset_index_batch_factory_entries_for_test() {
    INDEX_BATCH_FACTORY_ENTRIES.with(|entries| entries.set(0));
}

#[cfg(test)]
pub(crate) fn index_batch_factory_entries_for_test() -> usize {
    INDEX_BATCH_FACTORY_ENTRIES.with(std::cell::Cell::get)
}

/// Index the provided filesystem paths and return a list of [`Action`]s.
///
/// This compatibility helper exhausts the batched iterator into a single
/// vector; prefer [`index_paths_batched`] when possible.
pub fn index_paths(paths: &[String]) -> anyhow::Result<Vec<Action>> {
    let mut results = Vec::new();
    for batch in index_paths_batched(paths, IndexOptions::default()) {
        results.extend(batch?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::{IndexOptions, index_paths_batched};
    use crate::performance::{Metric, workloads};
    use std::cell::RefCell;

    fn exhaust(paths: &[String], options: IndexOptions) -> Vec<crate::actions::Action> {
        let mut actions = Vec::new();
        for batch in index_paths_batched(paths, options) {
            actions.extend(batch.expect("synthetic index tree traversal succeeds"));
        }
        actions
    }

    #[test]
    #[ignore = "opt-in Track A workload benchmark; set MULTI_LAUNCHER_PERF=1 before the process"]
    fn track_a_benchmark_index_lazy_traversal() {
        let workspace = workloads::IsolatedWorkspace::new();

        for count in workloads::selected_sizes(&[16, 1_000, 10_000]) {
            let root = workspace.root().join(format!("index-root-{count}"));
            let fixture = workloads::create_index_tree(&root, 0x494e_4445_585f_41, count);
            let root_string = root.to_string_lossy().into_owned();
            let roots = vec![root_string.clone()];
            let options = IndexOptions::default();
            let iterator_slot = RefCell::new(None);

            let (timing, measured_count) = workloads::measure(
                workloads::INDEX_WARMUPS,
                |_, _| {
                    *iterator_slot.borrow_mut() = Some(index_paths_batched(&roots, options));
                },
                || {
                    let iterator = iterator_slot
                        .borrow_mut()
                        .take()
                        .expect("setup creates a fresh lazy traversal");
                    let mut visited = 0_usize;
                    for batch in iterator {
                        let batch = batch.expect("synthetic index traversal has no errors");
                        visited += batch.len();
                        std::hint::black_box(batch);
                    }
                    visited
                },
            );
            assert_eq!(
                measured_count, count,
                "each measured scan exhausts the tree"
            );
            let metrics = workloads::metrics_for(&[Metric::IndexScan]);
            assert_eq!(metrics.len(), 1);
            assert_eq!(
                metrics[0].work_units,
                (count * workloads::SAMPLE_COUNT) as u64
            );
            assert_eq!(metrics[0].completed, workloads::SAMPLE_COUNT as u64);
            assert_eq!(metrics[0].errors, 0);
            assert_eq!(metrics[0].abandoned, 0);

            let actions = exhaust(&roots, options);
            assert_eq!(actions.len(), count);
            let membership_signature = workloads::index_actions_signature(&root, &actions);
            let order_signature = workloads::index_actions_order_signature(&root, &actions);

            let duplicate_roots = vec![root_string.clone(), root_string.clone()];
            let duplicate_actions = exhaust(&duplicate_roots, options);
            assert_eq!(
                duplicate_actions.len(),
                count,
                "duplicate roots are deduplicated"
            );
            assert_eq!(
                workloads::index_actions_signature(&root, &duplicate_actions),
                membership_signature
            );

            let max_items = (count / 2).max(1);
            let capped_actions = exhaust(
                &roots,
                IndexOptions {
                    batch_size: 127,
                    max_items,
                },
            );
            assert_eq!(capped_actions.len(), max_items);
            let capped_ids = capped_actions
                .iter()
                .map(|action| action.action.as_str())
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(capped_ids.len(), capped_actions.len());

            workloads::emit_summary(
                &format!("index-{count}-fresh-exhaustion"),
                "actual lazy iterator traversal; debug-test warm-cache filesystem; iterator construction excluded",
                fixture.with_output_signatures(Some(membership_signature), Some(order_signature)),
                timing,
                &metrics,
            );
        }
        drop(workspace);
    }
}
