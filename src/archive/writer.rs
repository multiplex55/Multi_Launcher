//! Bounded, handle-relative ZIP-entry writer. This module has no GUI or
//! command integration; a run owns one worker until its request, stage, and
//! held-directory resources are retired.

#![allow(dead_code)] // M3-B2 establishes the owner before M3-C activates it.

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
#[cfg(test)]
use std::time::Duration;

use crate::file_operations::ScanCancellation;

use super::ArchiveCatalog;
use super::crc32::Crc32;
use super::model::{ArchiveEntry, ArchiveEntryId, ArchiveRevision, EntryCompression, EntryKind};
use super::native::{
    NativeArchiveDestination, NativeArchiveError, NativeArchiveSource, OwnedStage,
};
use super::native_io::ArchiveIoControl;
use super::plan::{ExtractionPlan, PlannedOutput};

const CHUNK_BYTES: usize = 64 * 1024;
const CENTRAL_FIXED_BYTES: usize = 46;
const LOCAL_FIXED_BYTES: usize = 30;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;
const ZIP64_EXTRA_ID: u16 = 0x0001;
const FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;
static NEXT_RUN_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveWritePhase {
    Queued,
    Validating,
    Reading,
    Writing,
    Flushing,
    Publishing,
    Cancelling,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveWriteStatus {
    Completed,
    Cancelled,
    Partial,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveWriteFailureKind {
    Cancelled,
    InvalidPlan,
    SourceChanged,
    UnsupportedZipLayout,
    TruncatedPayload,
    IntegrityMismatch,
    QuotaExceeded,
    NativeIo,
    Allocation,
    WorkerPanicked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArchiveWriteProgress {
    pub run_id: u64,
    pub revision: ArchiveRevision,
    pub generation: u64,
    pub phase: ArchiveWritePhase,
    pub current_entry: Option<ArchiveEntryId>,
    pub published_count: usize,
    pub actual_bytes_written: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArchiveWriteReport {
    pub run_id: u64,
    pub revision: ArchiveRevision,
    pub generation: u64,
    pub status: ArchiveWriteStatus,
    pub phase: ArchiveWritePhase,
    pub published_entries: Vec<ArchiveEntryId>,
    pub failed_entry: Option<ArchiveEntryId>,
    pub cancelled_entry: Option<ArchiveEntryId>,
    pub unstarted_entries: Vec<ArchiveEntryId>,
    pub created_directories: usize,
    pub actual_bytes_written: u64,
    pub failure: Option<(ArchiveWriteFailureKind, String)>,
    pub cleanup_failed: bool,
}

#[derive(Clone)]
pub(crate) struct ArchiveWriteJobControl {
    shared: Arc<JobShared>,
}

struct JobShared {
    io: ArchiveIoControl,
    publication: Mutex<PublicationState>,
    progress: Mutex<ArchiveWriteProgress>,
    test_hooks: Mutex<WriterTestHooks>,
}

#[derive(Default)]
struct PublicationState {
    cancel_requested: bool,
    admitted: bool,
    retired: bool,
}

pub(crate) struct ArchiveWriteJob {
    control: ArchiveWriteJobControl,
    worker: Option<JoinHandle<ArchiveWriteReport>>,
}

#[derive(Debug)]
struct PipelineFailure {
    kind: ArchiveWriteFailureKind,
    entry: Option<ArchiveEntryId>,
    message: String,
    cleanup_failed: bool,
}

#[derive(Clone, Copy, Debug)]
struct EntryLayout {
    id: ArchiveEntryId,
    data_offset: u64,
    data_len: u64,
    output_len: u64,
    compression: EntryCompression,
    record_end: u64,
    crc32: u32,
}

#[derive(Default)]
struct WriterTestHooks {
    #[cfg(test)]
    before_flush: Option<Arc<WriterTestGate>>,
    #[cfg(test)]
    after_flush: Option<Arc<WriterTestGate>>,
    #[cfg(test)]
    before_publication: Option<Arc<WriterTestGate>>,
    #[cfg(test)]
    before_next_entry: Option<Arc<WriterTestGate>>,
    #[cfg(test)]
    after_payload_chunk: Option<Arc<WriterTestGate>>,
}

#[cfg(test)]
pub(super) struct WriterTestGate {
    reached: SyncSender<()>,
    resume: Mutex<Receiver<()>>,
}

#[derive(Default)]
struct Zip64Values {
    uncompressed: Option<u64>,
    compressed: Option<u64>,
    local_offset: Option<u64>,
    disk_start: Option<u32>,
}

impl ArchiveWriteJobControl {
    fn new(
        io: ArchiveIoControl,
        progress: ArchiveWriteProgress,
        test_hooks: WriterTestHooks,
    ) -> Self {
        Self {
            shared: Arc::new(JobShared {
                io,
                publication: Mutex::new(PublicationState::default()),
                progress: Mutex::new(progress),
                test_hooks: Mutex::new(test_hooks),
            }),
        }
    }

    pub(crate) fn cancel(&self) {
        {
            let mut publication = lock(&self.shared.publication);
            if publication.retired {
                return;
            }
            publication.cancel_requested = true;
            if !publication.admitted {
                self.set_phase(ArchiveWritePhase::Cancelling);
            }
        }
        self.shared.io.cancel();
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.shared.io.is_cancelled()
    }

    pub(crate) fn progress(&self) -> ArchiveWriteProgress {
        lock(&self.shared.progress).clone()
    }

    fn set_phase(&self, phase: ArchiveWritePhase) {
        lock(&self.shared.progress).phase = phase;
    }

    fn set_entry(&self, entry: Option<ArchiveEntryId>) {
        lock(&self.shared.progress).current_entry = entry;
    }

    fn record_written(&self, bytes: u64) -> Result<(), PipelineFailure> {
        let mut progress = lock(&self.shared.progress);
        progress.actual_bytes_written = progress
            .actual_bytes_written
            .checked_add(bytes)
            .ok_or_else(|| {
                PipelineFailure::new(
                    ArchiveWriteFailureKind::QuotaExceeded,
                    progress.current_entry,
                    "actual output byte counter overflow",
                )
            })?;
        Ok(())
    }

    fn record_published(&self) {
        lock(&self.shared.progress).published_count += 1;
    }

    fn admit_publication(&self) -> bool {
        let mut publication = lock(&self.shared.publication);
        if publication.cancel_requested || self.shared.io.is_cancelled() {
            return false;
        }
        publication.admitted = true;
        self.set_phase(ArchiveWritePhase::Publishing);
        true
    }

    fn finish_publication(&self) {
        let mut publication = lock(&self.shared.publication);
        publication.admitted = false;
        if publication.cancel_requested {
            self.set_phase(ArchiveWritePhase::Cancelling);
        }
    }

    fn mark_retired(&self) {
        let mut publication = lock(&self.shared.publication);
        publication.retired = true;
        self.set_phase(ArchiveWritePhase::Retired);
    }

    fn io(&self) -> &ArchiveIoControl {
        &self.shared.io
    }

    fn reach_test_flush_gate(&self, before_flush: bool) {
        #[cfg(test)]
        {
            let gate = {
                let hooks = lock(&self.shared.test_hooks);
                if before_flush {
                    hooks.before_flush.clone()
                } else {
                    hooks.after_flush.clone()
                }
            };
            if let Some(gate) = gate {
                gate.reach_and_wait();
            }
        }
        #[cfg(not(test))]
        let _ = before_flush;
    }

    fn reach_test_publication_gate(&self) {
        #[cfg(test)]
        {
            let gate = lock(&self.shared.test_hooks).before_publication.clone();
            if let Some(gate) = gate {
                gate.reach_and_wait();
            }
        }
    }

    fn reach_test_before_next_entry(&self) {
        #[cfg(test)]
        {
            let gate = lock(&self.shared.test_hooks).before_next_entry.take();
            if let Some(gate) = gate {
                gate.reach_and_wait();
            }
        }
    }

    fn reach_test_after_payload_chunk(&self) {
        #[cfg(test)]
        {
            let gate = lock(&self.shared.test_hooks).after_payload_chunk.take();
            if let Some(gate) = gate {
                gate.reach_and_wait();
            }
        }
    }
}

impl ArchiveWriteJob {
    pub(crate) fn start(
        source: NativeArchiveSource,
        catalog: ArchiveCatalog,
        plan: ExtractionPlan,
        destination: NativeArchiveDestination,
        cancellation: ScanCancellation,
    ) -> io::Result<Self> {
        Self::start_shared(source, Arc::new(catalog), plan, destination, cancellation)
    }

    pub(crate) fn start_shared(
        source: NativeArchiveSource,
        catalog: Arc<ArchiveCatalog>,
        plan: ExtractionPlan,
        destination: NativeArchiveDestination,
        cancellation: ScanCancellation,
    ) -> io::Result<Self> {
        Self::start_with_shared_control_and_hooks(
            source,
            catalog,
            plan,
            destination,
            ArchiveIoControl::new(cancellation),
            WriterTestHooks::default(),
        )
    }

    #[cfg(test)]
    pub(super) fn start_shared_with_before_flush_gate(
        source: NativeArchiveSource,
        catalog: Arc<ArchiveCatalog>,
        plan: ExtractionPlan,
        destination: NativeArchiveDestination,
        cancellation: ScanCancellation,
        gate: Arc<WriterTestGate>,
    ) -> io::Result<Self> {
        Self::start_with_shared_control_and_hooks(
            source,
            catalog,
            plan,
            destination,
            ArchiveIoControl::new(cancellation),
            WriterTestHooks {
                before_flush: Some(gate),
                ..WriterTestHooks::default()
            },
        )
    }

    fn start_with_control(
        source: NativeArchiveSource,
        catalog: ArchiveCatalog,
        plan: ExtractionPlan,
        destination: NativeArchiveDestination,
        io: ArchiveIoControl,
    ) -> io::Result<Self> {
        Self::start_with_control_and_hooks(
            source,
            catalog,
            plan,
            destination,
            io,
            WriterTestHooks::default(),
        )
    }

    fn start_with_control_and_hooks(
        source: NativeArchiveSource,
        catalog: ArchiveCatalog,
        plan: ExtractionPlan,
        destination: NativeArchiveDestination,
        io: ArchiveIoControl,
        test_hooks: WriterTestHooks,
    ) -> io::Result<Self> {
        Self::start_with_shared_control_and_hooks(
            source,
            Arc::new(catalog),
            plan,
            destination,
            io,
            test_hooks,
        )
    }

    fn start_with_shared_control_and_hooks(
        source: NativeArchiveSource,
        catalog: Arc<ArchiveCatalog>,
        plan: ExtractionPlan,
        destination: NativeArchiveDestination,
        io: ArchiveIoControl,
        test_hooks: WriterTestHooks,
    ) -> io::Result<Self> {
        let run_id = NEXT_RUN_ID.fetch_add(1, Ordering::Relaxed);
        let revision = plan.source().revision;
        let progress = ArchiveWriteProgress {
            run_id,
            revision,
            generation: run_id,
            phase: ArchiveWritePhase::Queued,
            current_entry: None,
            published_count: 0,
            actual_bytes_written: 0,
        };
        let control = ArchiveWriteJobControl::new(io, progress, test_hooks);
        let worker_control = control.clone();
        let worker = thread::Builder::new()
            .name(format!("archive-write-{run_id}"))
            .spawn(move || {
                let mut report = ArchiveWriteReport::empty(run_id, revision);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    execute_pipeline(
                        source,
                        &catalog,
                        plan,
                        destination,
                        &worker_control,
                        &mut report,
                    )
                }));
                if let Err(payload) = result {
                    let _ = payload;
                    let completed_count = report
                        .published_entries
                        .len()
                        .min(report.unstarted_entries.len());
                    report.unstarted_entries.drain(..completed_count);
                    report.failure = Some((
                        ArchiveWriteFailureKind::WorkerPanicked,
                        "archive writer worker panicked".to_owned(),
                    ));
                    report.status = if report.published_entries.is_empty() {
                        ArchiveWriteStatus::Failed
                    } else {
                        ArchiveWriteStatus::Partial
                    };
                }
                let progress = worker_control.progress();
                report.phase = progress.phase;
                report.actual_bytes_written = progress.actual_bytes_written;
                report.generation = progress.generation;
                report
            })?;
        Ok(Self {
            control,
            worker: Some(worker),
        })
    }

    #[cfg(test)]
    fn start_with_test_io_and_gates(
        source: NativeArchiveSource,
        catalog: ArchiveCatalog,
        plan: ExtractionPlan,
        destination: NativeArchiveDestination,
        io: ArchiveIoControl,
        before_flush: Option<Arc<WriterTestGate>>,
        after_flush: Option<Arc<WriterTestGate>>,
        before_publication: Option<Arc<WriterTestGate>>,
    ) -> io::Result<Self> {
        Self::start_with_control_and_hooks(
            source,
            catalog,
            plan,
            destination,
            io,
            WriterTestHooks {
                before_flush,
                after_flush,
                before_publication,
                ..WriterTestHooks::default()
            },
        )
    }

    pub(crate) fn control(&self) -> ArchiveWriteJobControl {
        self.control.clone()
    }

    pub(crate) fn cancel(&self) {
        self.control.cancel();
    }

    pub(crate) fn progress(&self) -> ArchiveWriteProgress {
        self.control.progress()
    }

    pub(crate) fn try_join_finished(&mut self) -> Option<ArchiveWriteReport> {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            Some(self.join_inner())
        } else {
            None
        }
    }

    pub(crate) fn join(mut self) -> ArchiveWriteReport {
        self.join_inner()
    }

    fn join_inner(&mut self) -> ArchiveWriteReport {
        let Some(worker) = self.worker.take() else {
            let progress = self.control.progress();
            return ArchiveWriteReport::empty(progress.run_id, progress.revision);
        };
        let mut report = match worker.join() {
            Ok(report) => report,
            Err(_) => {
                let progress = self.control.progress();
                ArchiveWriteReport {
                    run_id: progress.run_id,
                    revision: progress.revision,
                    generation: progress.generation,
                    status: if progress.published_count == 0 {
                        ArchiveWriteStatus::Failed
                    } else {
                        ArchiveWriteStatus::Partial
                    },
                    phase: progress.phase,
                    published_entries: Vec::new(),
                    failed_entry: None,
                    cancelled_entry: None,
                    unstarted_entries: Vec::new(),
                    created_directories: 0,
                    actual_bytes_written: progress.actual_bytes_written,
                    failure: Some((
                        ArchiveWriteFailureKind::WorkerPanicked,
                        "archive writer worker panicked".to_owned(),
                    )),
                    cleanup_failed: false,
                }
            }
        };
        self.control.mark_retired();
        report.phase = ArchiveWritePhase::Retired;
        let progress = self.control.progress();
        report.actual_bytes_written = progress.actual_bytes_written;
        report.generation = progress.generation;
        report
    }
}

impl Drop for ArchiveWriteJob {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.control.cancel();
            let _ = self.join_inner();
        }
    }
}

impl ArchiveWriteReport {
    fn empty(run_id: u64, revision: ArchiveRevision) -> Self {
        Self {
            run_id,
            revision,
            generation: run_id,
            status: ArchiveWriteStatus::Failed,
            phase: ArchiveWritePhase::Queued,
            published_entries: Vec::new(),
            failed_entry: None,
            cancelled_entry: None,
            unstarted_entries: Vec::new(),
            created_directories: 0,
            actual_bytes_written: 0,
            failure: None,
            cleanup_failed: false,
        }
    }
}

impl PipelineFailure {
    fn new(
        kind: ArchiveWriteFailureKind,
        entry: Option<ArchiveEntryId>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            entry,
            message: message.into(),
            cleanup_failed: false,
        }
    }

    fn native(error: NativeArchiveError, entry: Option<ArchiveEntryId>) -> Self {
        let cleanup_failed = error.cleanup_failed();
        let mut failure = Self::new(ArchiveWriteFailureKind::NativeIo, entry, error.to_string());
        failure.cleanup_failed = cleanup_failed;
        failure
    }

    fn io(error: io::Error, entry: Option<ArchiveEntryId>) -> Self {
        Self::new(ArchiveWriteFailureKind::NativeIo, entry, error.to_string())
    }

    fn cancelled(entry: Option<ArchiveEntryId>) -> Self {
        Self::new(ArchiveWriteFailureKind::Cancelled, entry, "cancelled")
    }
}

fn execute_pipeline(
    mut source: NativeArchiveSource,
    catalog: &ArchiveCatalog,
    plan: ExtractionPlan,
    destination: NativeArchiveDestination,
    control: &ArchiveWriteJobControl,
    report: &mut ArchiveWriteReport,
) {
    let files = match collect_file_outputs(&plan) {
        Ok(files) => files,
        Err(error) => {
            report.failure = Some((error.kind, error.message));
            report.cleanup_failed = error.cleanup_failed;
            return;
        }
    };
    if report
        .unstarted_entries
        .try_reserve_exact(files.len())
        .is_err()
        || report
            .published_entries
            .try_reserve_exact(files.len())
            .is_err()
    {
        report.failure = Some((
            ArchiveWriteFailureKind::Allocation,
            "could not reserve bounded archive report storage".to_owned(),
        ));
        return;
    }
    report
        .unstarted_entries
        .extend(files.iter().filter_map(|output| output.source_entry()));

    control.set_phase(ArchiveWritePhase::Validating);
    if control.is_cancelled() {
        report.status = ArchiveWriteStatus::Cancelled;
        return;
    }
    if catalog.source() != plan.source()
        || source.archive_source() != Some(catalog.source())
        || !plan.policy().no_overwrite
    {
        set_pipeline_failure(
            report,
            PipelineFailure::new(
                ArchiveWriteFailureKind::InvalidPlan,
                None,
                "plan, catalog, and held source do not describe one no-replace revision",
            ),
        );
        return;
    }
    let actual_cancellation = control.io().cancellation_token();
    if control.is_cancelled() {
        report.status = ArchiveWriteStatus::Cancelled;
        return;
    }
    // The read adapter and worker share one cancellation owner. Cataloging
    // revalidates both the revision and physical central-directory ordinals
    // against the exact held source before any destination mutation.
    if let Err(error) = source.verify_catalog_with_io(catalog, &actual_cancellation, control.io()) {
        let failure = if control.is_cancelled() {
            PipelineFailure::cancelled(None)
        } else if error.kind() == io::ErrorKind::InvalidData {
            PipelineFailure::new(
                ArchiveWriteFailureKind::SourceChanged,
                None,
                error.to_string(),
            )
        } else {
            PipelineFailure::native(error, None)
        };
        set_pipeline_failure(report, failure);
        return;
    }

    control.set_phase(ArchiveWritePhase::Reading);
    let layouts = match validate_selected_entries(&source, catalog, &plan, control.io()) {
        Ok(layouts) => layouts,
        Err(error) => {
            set_pipeline_failure(report, error);
            return;
        }
    };
    if control.is_cancelled() {
        report.status = ArchiveWriteStatus::Cancelled;
        return;
    }
    if let Err(error) = destination.verify_identity() {
        set_pipeline_failure(report, PipelineFailure::native(error, None));
        return;
    }

    for output in plan.directories() {
        if control.is_cancelled() {
            report.status = ArchiveWriteStatus::Cancelled;
            return;
        }
        let (_, created) = match destination.ensure_directory_path(output.path()) {
            Ok(result) => result,
            Err(error) => {
                set_pipeline_failure(
                    report,
                    PipelineFailure::native(error, output.source_entry()),
                );
                return;
            }
        };
        report.created_directories = report.created_directories.saturating_add(created);
    }

    for (index, (output, layout)) in files.iter().zip(layouts.iter()).enumerate() {
        let entry_id = output.source_entry();
        control.set_entry(entry_id);
        if control.is_cancelled() {
            set_pipeline_failure(report, PipelineFailure::cancelled(entry_id));
            set_unstarted_entries(report, &files, index + 1);
            return;
        }
        match write_one_entry(
            &source,
            &destination,
            output,
            *layout,
            plan.policy().max_actual_entry_output_bytes,
            plan.policy().max_actual_total_output_bytes,
            control,
            report,
        ) {
            Ok(published_id) => {
                report.published_entries.push(published_id);
                control.record_published();
            }
            Err(error) => {
                if error.kind == ArchiveWriteFailureKind::Cancelled {
                    report.cancelled_entry = error.entry.or(entry_id);
                } else {
                    report.failed_entry = error.entry.or(entry_id);
                }
                set_pipeline_failure(report, error);
                set_unstarted_entries(report, &files, index + 1);
                return;
            }
        }
        if control.is_cancelled() && index + 1 < files.len() {
            report.status = ArchiveWriteStatus::Partial;
            report.cancelled_entry = files.get(index + 1).and_then(|next| next.source_entry());
            set_unstarted_entries(report, &files, index + 2);
            return;
        }
        if index + 1 < files.len() {
            control.reach_test_before_next_entry();
        }
    }
    control.set_entry(None);
    report.status = ArchiveWriteStatus::Completed;
    report.unstarted_entries.clear();
}

fn collect_file_outputs(plan: &ExtractionPlan) -> Result<Vec<&PlannedOutput>, PipelineFailure> {
    let count = plan.files().count();
    let mut files = Vec::new();
    files.try_reserve_exact(count).map_err(|_| {
        PipelineFailure::new(
            ArchiveWriteFailureKind::Allocation,
            None,
            "could not reserve bounded file plan view",
        )
    })?;
    files.extend(plan.files());
    Ok(files)
}

fn validate_selected_entries(
    source: &NativeArchiveSource,
    catalog: &ArchiveCatalog,
    plan: &ExtractionPlan,
    io: &ArchiveIoControl,
) -> Result<Vec<EntryLayout>, PipelineFailure> {
    let files = collect_file_outputs(plan)?;
    let mut offsets = Vec::new();
    offsets
        .try_reserve_exact(catalog.entries().len())
        .map_err(|_| {
            PipelineFailure::new(
                ArchiveWriteFailureKind::Allocation,
                None,
                "could not reserve bounded physical-entry index",
            )
        })?;
    offsets.extend(
        catalog
            .entries()
            .iter()
            .map(|entry| (entry.local_header_offset(), entry.id())),
    );
    offsets.sort_unstable_by_key(|(offset, id)| (*offset, *id));

    // Establish a unique nonoverlapping physical record boundary for every
    // catalog entry, including entries omitted from the selection. Otherwise
    // an omitted earlier record could claim bytes containing a selected local
    // header and make the selected payload interpretation ambiguous.
    let mut physical_layouts = Vec::new();
    physical_layouts
        .try_reserve_exact(catalog.entries().len())
        .map_err(|_| {
            PipelineFailure::new(
                ArchiveWriteFailureKind::Allocation,
                None,
                "could not reserve bounded physical-entry layouts",
            )
        })?;
    for (index, entry) in catalog.entries().iter().enumerate() {
        let id = entry.id();
        if id.revision() != catalog.source().revision
            || usize::try_from(id.ordinal()).ok() != Some(index)
        {
            return Err(PipelineFailure::new(
                ArchiveWriteFailureKind::SourceChanged,
                Some(id),
                "catalog physical ordinals are not a contiguous source revision",
            ));
        }
        physical_layouts.push(validate_entry_layout(source, catalog, entry, &offsets, io)?);
    }

    let mut layouts = Vec::new();
    layouts.try_reserve_exact(files.len()).map_err(|_| {
        PipelineFailure::new(
            ArchiveWriteFailureKind::Allocation,
            None,
            "could not reserve bounded selected-entry list",
        )
    })?;

    let mut spans = Vec::new();
    spans.try_reserve_exact(files.len()).map_err(|_| {
        PipelineFailure::new(
            ArchiveWriteFailureKind::Allocation,
            None,
            "could not reserve bounded entry span list",
        )
    })?;

    for output in plan.outputs() {
        let Some(id) = output.source_entry() else {
            if output.kind() != EntryKind::Directory {
                return Err(PipelineFailure::new(
                    ArchiveWriteFailureKind::InvalidPlan,
                    None,
                    "file output is missing its physical source identity",
                ));
            }
            continue;
        };
        let entry = catalog.entry(id).ok_or_else(|| {
            PipelineFailure::new(
                ArchiveWriteFailureKind::InvalidPlan,
                Some(id),
                "plan references an unknown physical entry ordinal",
            )
        })?;
        if id.revision() != catalog.source().revision
            || usize::try_from(id.ordinal())
                .ok()
                .and_then(|index| catalog.entries().get(index))
                != Some(entry)
            || output.kind() != entry.kind()
        {
            return Err(PipelineFailure::new(
                ArchiveWriteFailureKind::InvalidPlan,
                Some(id),
                "planned output does not match its catalog ordinal",
            ));
        }
        if entry.kind() == EntryKind::File {
            if !entry.is_supported()
                || (entry.compression() == EntryCompression::Stored
                    && entry.compressed_size() != entry.uncompressed_size())
                || output.compressed_size() != entry.compressed_size()
                || output.declared_output_size() != entry.uncompressed_size()
            {
                return Err(PipelineFailure::new(
                    ArchiveWriteFailureKind::UnsupportedZipLayout,
                    Some(id),
                    "selected file has unsupported compression, flags, or sizes",
                ));
            }
            if entry.uncompressed_size() > plan.policy().max_actual_entry_output_bytes {
                return Err(PipelineFailure::new(
                    ArchiveWriteFailureKind::QuotaExceeded,
                    Some(id),
                    "declared entry size exceeds the captured actual-output quota",
                ));
            }
            let physical_index = usize::try_from(id.ordinal()).map_err(|_| {
                PipelineFailure::new(
                    ArchiveWriteFailureKind::InvalidPlan,
                    Some(id),
                    "selected physical ordinal does not fit the bounded catalog",
                )
            })?;
            let layout = physical_layouts
                .get(physical_index)
                .filter(|layout| layout.id == id)
                .ok_or_else(|| {
                    PipelineFailure::new(
                        ArchiveWriteFailureKind::InvalidPlan,
                        Some(id),
                        "selected physical layout is missing from the validated catalog",
                    )
                })?;
            layouts.push(*layout);
        } else {
            if !entry.is_supported() || entry.uncompressed_size() != 0 {
                return Err(unsupported(
                    id,
                    "directory entry must have a supported empty payload",
                ));
            }
            let layout = physical_layouts[usize::try_from(id.ordinal())
                .map_err(|_| unsupported(id, "directory ordinal is out of range"))?];
            decode_payload(source, layout, io, || {}, |_| Ok(()))?;
        }
    }

    spans.extend(
        layouts
            .iter()
            .map(|layout| (layout.data_offset, layout.record_end, layout.id)),
    );
    spans.sort_unstable_by_key(|(start, _, id)| (*start, *id));
    for pair in spans.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(PipelineFailure::new(
                ArchiveWriteFailureKind::UnsupportedZipLayout,
                Some(pair[1].2),
                "selected ZIP payload spans overlap",
            ));
        }
    }
    Ok(layouts)
}

pub(super) fn validate_plan_for_preview(
    source: &NativeArchiveSource,
    catalog: &ArchiveCatalog,
    plan: &ExtractionPlan,
    io: &ArchiveIoControl,
) -> Result<(), (ArchiveWriteFailureKind, Option<ArchiveEntryId>)> {
    validate_selected_entries(source, catalog, plan, io)
        .map(|_| ())
        .map_err(|failure| (failure.kind, failure.entry))
}

fn validate_entry_layout(
    source: &NativeArchiveSource,
    catalog: &ArchiveCatalog,
    entry: &ArchiveEntry,
    offsets: &[(u64, ArchiveEntryId)],
    io: &ArchiveIoControl,
) -> Result<EntryLayout, PipelineFailure> {
    let id = entry.id();
    if io.is_cancelled() {
        return Err(PipelineFailure::cancelled(Some(id)));
    }
    let central_offset = entry.central_header_offset();
    let mut central = [0_u8; CENTRAL_FIXED_BYTES];
    read_exact_at(source, io, central_offset, &mut central, Some(id))?;
    if le_u32(&central, 0) != CENTRAL_SIGNATURE
        || le_u16(&central, 4) != entry.version_made_by()
        || le_u16(&central, 8) != entry.flags()
        || le_u16(&central, 10) != entry.compression().method_code()
        || le_u32(&central, 16) != entry.crc32()
        || le_u32(&central, 38) != entry.external_attributes()
    {
        return Err(PipelineFailure::new(
            ArchiveWriteFailureKind::SourceChanged,
            Some(id),
            "central record no longer matches the selected catalog entry",
        ));
    }
    let version_needed = le_u16(&central, 6);
    let compressed32 = le_u32(&central, 20);
    let uncompressed32 = le_u32(&central, 24);
    let name_len = usize::from(le_u16(&central, 28));
    let extra_len = usize::from(le_u16(&central, 30));
    let comment_len = usize::from(le_u16(&central, 32));
    let disk_start16 = le_u16(&central, 34);
    let local_offset32 = le_u32(&central, 42);
    let central_record_len = CENTRAL_FIXED_BYTES
        .checked_add(name_len)
        .and_then(|len| len.checked_add(extra_len))
        .and_then(|len| len.checked_add(comment_len))
        .ok_or_else(|| unsupported(id, "central record length overflow"))?;
    let central_record_end = central_offset
        .checked_add(central_record_len as u64)
        .ok_or_else(|| unsupported(id, "central record end overflow"))?;
    if central_record_end > catalog.source().central_directory_end {
        return Err(unsupported(
            id,
            "central record exceeds the admitted directory",
        ));
    }
    let mut name = vec![0_u8; name_len];
    read_exact_at(
        source,
        io,
        central_offset + CENTRAL_FIXED_BYTES as u64,
        &mut name,
        Some(id),
    )?;
    if name != entry.name().raw_bytes() {
        return Err(PipelineFailure::new(
            ArchiveWriteFailureKind::SourceChanged,
            Some(id),
            "central entry name changed after cataloging",
        ));
    }
    let mut extra = vec![0_u8; extra_len];
    read_exact_at(
        source,
        io,
        central_offset + CENTRAL_FIXED_BYTES as u64 + name_len as u64,
        &mut extra,
        Some(id),
    )?;
    let need_c = compressed32 == u32::MAX;
    let need_u = uncompressed32 == u32::MAX;
    let need_off = local_offset32 == u32::MAX;
    let need_disk = disk_start16 == u16::MAX;
    if (need_c || need_u || need_off || need_disk) && version_needed < 45 {
        return Err(unsupported(id, "ZIP64 fields require version 4.5 or later"));
    }
    let central_zip64 = parse_zip64_extra(&extra, need_u, need_c, need_off, need_disk, id)?;
    let compressed = central_zip64
        .compressed
        .unwrap_or_else(|| u64::from(compressed32));
    let uncompressed = central_zip64
        .uncompressed
        .unwrap_or_else(|| u64::from(uncompressed32));
    let local_offset = central_zip64
        .local_offset
        .unwrap_or_else(|| u64::from(local_offset32));
    if central_zip64.disk_start.unwrap_or(u32::from(disk_start16)) != 0
        || compressed != entry.compressed_size()
        || uncompressed != entry.uncompressed_size()
        || local_offset != entry.local_header_offset()
    {
        return Err(PipelineFailure::new(
            ArchiveWriteFailureKind::SourceChanged,
            Some(id),
            "central ZIP64 values disagree with the catalog revision",
        ));
    }

    let local_offset = entry.local_header_offset();
    let physical_index = offsets
        .binary_search_by_key(&(local_offset, id), |(offset, candidate)| {
            (*offset, *candidate)
        })
        .map_err(|_| {
            PipelineFailure::new(
                ArchiveWriteFailureKind::SourceChanged,
                Some(id),
                "selected physical local-header ordinal is missing",
            )
        })?;
    let next_local = offsets
        .get(physical_index + 1)
        .map(|(offset, _)| *offset)
        .unwrap_or(catalog.source().central_directory_offset);
    if next_local <= local_offset || next_local > catalog.source().central_directory_offset {
        return Err(unsupported(id, "physical local-header ordinals overlap"));
    }
    let mut local = [0_u8; LOCAL_FIXED_BYTES];
    read_exact_at(source, io, local_offset, &mut local, Some(id))?;
    if le_u32(&local, 0) != LOCAL_SIGNATURE
        || le_u16(&local, 4) != version_needed
        || le_u16(&local, 6) != entry.flags()
        || le_u16(&local, 8) != entry.compression().method_code()
    {
        return Err(PipelineFailure::new(
            ArchiveWriteFailureKind::SourceChanged,
            Some(id),
            "local record signature, version, flags, or method changed",
        ));
    }
    let local_crc = le_u32(&local, 14);
    let local_compressed32 = le_u32(&local, 18);
    let local_uncompressed32 = le_u32(&local, 22);
    let local_name_len = usize::from(le_u16(&local, 26));
    let local_extra_len = usize::from(le_u16(&local, 28));
    if local_name_len != name_len {
        return Err(unsupported(id, "local and central filename lengths differ"));
    }
    let local_header_len = LOCAL_FIXED_BYTES
        .checked_add(local_name_len)
        .and_then(|len| len.checked_add(local_extra_len))
        .ok_or_else(|| unsupported(id, "local header length overflow"))?;
    let data_offset = local_offset
        .checked_add(local_header_len as u64)
        .ok_or_else(|| unsupported(id, "local payload offset overflow"))?;
    let local_header_end = data_offset;
    if local_header_end > next_local || local_header_end > source.archive_len() {
        return Err(unsupported(
            id,
            "local header exceeds the next physical record",
        ));
    }
    let mut local_name = vec![0_u8; local_name_len];
    read_exact_at(
        source,
        io,
        local_offset + LOCAL_FIXED_BYTES as u64,
        &mut local_name,
        Some(id),
    )?;
    if local_name != name {
        return Err(PipelineFailure::new(
            ArchiveWriteFailureKind::SourceChanged,
            Some(id),
            "local and central filename bytes differ",
        ));
    }
    let mut local_extra = vec![0_u8; local_extra_len];
    read_exact_at(
        source,
        io,
        local_offset + LOCAL_FIXED_BYTES as u64 + local_name_len as u64,
        &mut local_extra,
        Some(id),
    )?;
    let local_need_c = local_compressed32 == u32::MAX;
    let local_need_u = local_uncompressed32 == u32::MAX;
    let has_descriptor = entry.flags() & FLAG_DATA_DESCRIPTOR != 0;
    // Streaming ZIP64 writers use zero local size placeholders and still emit
    // the two ZIP64 size slots. The central ZIP64 sentinels and descriptor
    // establish their meaning; validate each slot against the central values.
    let streaming_zip64 = has_descriptor
        && need_c
        && need_u
        && !local_need_c
        && !local_need_u
        && local_compressed32 == 0
        && local_uncompressed32 == 0
        && local_extra.len() == 20
        && le_u16(&local_extra, 0) == ZIP64_EXTRA_ID
        && le_u16(&local_extra, 2) == 16;
    let local_zip64 = parse_zip64_extra(
        &local_extra,
        local_need_u || streaming_zip64,
        local_need_c || streaming_zip64,
        false,
        false,
        id,
    )?;
    let local_compressed = local_zip64
        .compressed
        .unwrap_or_else(|| u64::from(local_compressed32));
    let local_uncompressed = local_zip64
        .uncompressed
        .unwrap_or_else(|| u64::from(local_uncompressed32));
    if has_descriptor {
        if local_crc != 0 && local_crc != entry.crc32()
            || local_compressed != 0 && local_compressed != compressed
            || local_uncompressed != 0 && local_uncompressed != uncompressed
        {
            return Err(unsupported(
                id,
                "descriptor-mode local placeholders disagree",
            ));
        }
    } else if local_crc != entry.crc32()
        || local_compressed != compressed
        || local_uncompressed != uncompressed
    {
        return Err(PipelineFailure::new(
            ArchiveWriteFailureKind::SourceChanged,
            Some(id),
            "local CRC or sizes disagree with the central record",
        ));
    }
    let data_len = entry.compressed_size();
    let data_end = data_offset
        .checked_add(data_len)
        .ok_or_else(|| unsupported(id, "payload end overflow"))?;
    if data_end > next_local || data_end > catalog.source().central_directory_offset {
        return Err(unsupported(id, "payload exceeds the next physical record"));
    }
    let record_end = if has_descriptor {
        validate_data_descriptor(
            source,
            io,
            id,
            data_end,
            next_local,
            entry.crc32(),
            compressed,
            uncompressed,
            need_c || need_u || local_need_c || local_need_u,
        )?
    } else {
        data_end
    };
    if record_end != next_local {
        return Err(unsupported(
            id,
            "entry record leaves unexplained bytes before the next physical record",
        ));
    }
    if record_end > catalog.source().central_directory_offset {
        return Err(unsupported(
            id,
            "entry record exceeds the central directory",
        ));
    }
    Ok(EntryLayout {
        id,
        data_offset,
        data_len,
        output_len: entry.uncompressed_size(),
        compression: entry.compression(),
        record_end,
        crc32: entry.crc32(),
    })
}

fn parse_zip64_extra(
    extra: &[u8],
    need_uncompressed: bool,
    need_compressed: bool,
    need_offset: bool,
    need_disk: bool,
    id: ArchiveEntryId,
) -> Result<Zip64Values, PipelineFailure> {
    let needs_zip64 = need_uncompressed || need_compressed || need_offset || need_disk;
    if !needs_zip64 {
        if !extra.is_empty() {
            return Err(unsupported(id, "unsupported or unexplained extra field"));
        }
        return Ok(Zip64Values::default());
    }
    if extra.len() < 4 || le_u16(extra, 0) != ZIP64_EXTRA_ID {
        return Err(unsupported(id, "required ZIP64 extra field is missing"));
    }
    let field_len = usize::from(le_u16(extra, 2));
    if field_len.checked_add(4) != Some(extra.len()) {
        return Err(unsupported(
            id,
            "ZIP64 extra field has unexplained trailing bytes",
        ));
    }
    let mut cursor = 4_usize;
    let mut values = Zip64Values::default();
    for (needed, slot) in [
        (need_uncompressed, &mut values.uncompressed),
        (need_compressed, &mut values.compressed),
        (need_offset, &mut values.local_offset),
    ] {
        if needed {
            let end = cursor
                .checked_add(8)
                .filter(|end| *end <= extra.len())
                .ok_or_else(|| unsupported(id, "ZIP64 extra field is truncated"))?;
            *slot = Some(le_u64(extra, cursor));
            cursor = end;
        }
    }
    if need_disk {
        let end = cursor
            .checked_add(4)
            .filter(|end| *end <= extra.len())
            .ok_or_else(|| unsupported(id, "ZIP64 disk field is truncated"))?;
        values.disk_start = Some(le_u32(extra, cursor));
        cursor = end;
    }
    if cursor != extra.len() {
        return Err(unsupported(id, "ZIP64 extra field has unexplained values"));
    }
    Ok(values)
}

fn validate_data_descriptor(
    source: &NativeArchiveSource,
    io: &ArchiveIoControl,
    id: ArchiveEntryId,
    offset: u64,
    bound: u64,
    crc: u32,
    compressed: u64,
    uncompressed: u64,
    zip64: bool,
) -> Result<u64, PipelineFailure> {
    let width = if zip64 { 8 } else { 4 };
    let descriptor_len = 4 + width * 2;
    let signed_len = descriptor_len + 4;
    let available = usize::try_from(bound.saturating_sub(offset)).unwrap_or(usize::MAX);
    if available < descriptor_len {
        return Err(unsupported(id, "data descriptor exceeds the next record"));
    }
    let mut bytes = vec![0_u8; signed_len.min(available)];
    read_exact_at(source, io, offset, &mut bytes, Some(id))?;
    let mut matches = Vec::with_capacity(2);
    if bytes.len() >= descriptor_len
        && le_u32(&bytes, 0) == crc
        && descriptor_sizes_match(&bytes, 4, width, compressed, uncompressed)
    {
        matches.push(offset + descriptor_len as u64);
    }
    if bytes.len() >= signed_len
        && le_u32(&bytes, 0) == DESCRIPTOR_SIGNATURE
        && le_u32(&bytes, 4) == crc
        && descriptor_sizes_match(&bytes, 8, width, compressed, uncompressed)
    {
        matches.push(offset + signed_len as u64);
    }
    if matches.len() != 1 {
        return Err(unsupported(id, "data descriptor is invalid or ambiguous"));
    }
    Ok(matches[0])
}

fn descriptor_sizes_match(
    bytes: &[u8],
    offset: usize,
    width: usize,
    compressed: u64,
    uncompressed: u64,
) -> bool {
    if width == 8 {
        le_u64(bytes, offset) == compressed && le_u64(bytes, offset + 8) == uncompressed
    } else {
        u64::from(le_u32(bytes, offset)) == compressed
            && u64::from(le_u32(bytes, offset + 4)) == uncompressed
    }
}

fn write_one_entry(
    source: &NativeArchiveSource,
    destination: &NativeArchiveDestination,
    output: &PlannedOutput,
    layout: EntryLayout,
    entry_limit: u64,
    total_limit: u64,
    control: &ArchiveWriteJobControl,
    report: &mut ArchiveWriteReport,
) -> Result<ArchiveEntryId, PipelineFailure> {
    let id = layout.id;
    let (parent, leaf, created_directories) = destination
        .open_parent_for_create(output.path())
        .map_err(|error| PipelineFailure::native(error, Some(id)))?;
    report.created_directories = report
        .created_directories
        .saturating_add(created_directories);
    if control.is_cancelled() {
        return Err(PipelineFailure::cancelled(Some(id)));
    }
    let stage = parent
        .create_stage()
        .map_err(|error| PipelineFailure::native(error, Some(id)))?;
    let result = write_stage_payload(source, &stage, layout, entry_limit, total_limit, control);
    if let Err(error) = result {
        return Err(abort_stage(stage, error));
    }
    if control.is_cancelled() {
        return Err(abort_stage(stage, PipelineFailure::cancelled(Some(id))));
    }
    if !control.admit_publication() {
        return Err(abort_stage(stage, PipelineFailure::cancelled(Some(id))));
    }
    // Admission is atomic under the short publication lock above. Release it
    // before the synchronous handle-relative rename so cancellation never
    // waits behind native finalization on the caller's thread.
    control.reach_test_publication_gate();
    let published = stage
        .publish(&leaf)
        .map_err(|error| PipelineFailure::native(error, Some(id)));
    control.finish_publication();
    published.map(|_| id)
}

fn write_stage_payload(
    source: &NativeArchiveSource,
    stage: &OwnedStage,
    layout: EntryLayout,
    entry_limit: u64,
    total_limit: u64,
    control: &ArchiveWriteJobControl,
) -> Result<(), PipelineFailure> {
    let mut stage_offset = 0_u64;
    decode_payload(
        source,
        layout,
        control.io(),
        || control.set_phase(ArchiveWritePhase::Reading),
        |bytes| {
            let mut position = 0;
            while position < bytes.len() {
                if control.is_cancelled() {
                    return Err(PipelineFailure::cancelled(Some(layout.id)));
                }
                let remaining = bytes.len() - position;
                let next_entry = stage_offset
                    .checked_add(remaining as u64)
                    .ok_or_else(|| quota_failure(layout.id, "entry byte counter overflow"))?;
                let next_total = control
                    .progress()
                    .actual_bytes_written
                    .checked_add(remaining as u64)
                    .ok_or_else(|| quota_failure(layout.id, "aggregate byte counter overflow"))?;
                if next_entry > entry_limit || next_total > total_limit {
                    return Err(quota_failure(
                        layout.id,
                        "actual output quota would be exceeded before the next write",
                    ));
                }
                control.set_phase(ArchiveWritePhase::Writing);
                let count = stage
                    .write_chunk(control.io(), stage_offset, &bytes[position..])
                    .map_err(|error| {
                        if control.is_cancelled() {
                            PipelineFailure::cancelled(Some(layout.id))
                        } else {
                            PipelineFailure::native(error, Some(layout.id))
                        }
                    })?;
                if count == 0 || count > remaining {
                    return Err(PipelineFailure::new(
                        ArchiveWriteFailureKind::NativeIo,
                        Some(layout.id),
                        "staged overlapped write made invalid progress",
                    ));
                }
                position += count;
                stage_offset += count as u64;
                control.record_written(count as u64)?;
            }
            control.reach_test_after_payload_chunk();
            Ok(())
        },
    )?;
    if control.is_cancelled() {
        return Err(PipelineFailure::cancelled(Some(layout.id)));
    }
    control.reach_test_flush_gate(true);
    if control.is_cancelled() {
        return Err(PipelineFailure::cancelled(Some(layout.id)));
    }
    control.set_phase(ArchiveWritePhase::Flushing);
    stage
        .flush_final()
        .map_err(|error| PipelineFailure::native(error, Some(layout.id)))?;
    control.reach_test_flush_gate(false);
    if control.is_cancelled() {
        return Err(PipelineFailure::cancelled(Some(layout.id)));
    }
    Ok(())
}

// Decode only the validated compressed span, without handing a pathname or
// unbounded reader to the ZIP library. Every plaintext chunk shares this
// size/CRC boundary before either a stage write or empty-directory check.
fn decode_payload(
    source: &NativeArchiveSource,
    layout: EntryLayout,
    io: &ArchiveIoControl,
    mut on_read: impl FnMut(),
    mut emit: impl FnMut(&[u8]) -> Result<(), PipelineFailure>,
) -> Result<(), PipelineFailure> {
    let mut inflater = match layout.compression {
        EntryCompression::Stored => None,
        EntryCompression::Deflated => Some(flate2::Decompress::new(false)),
        EntryCompression::Unsupported(_) => {
            return Err(unsupported(layout.id, "unsupported compression method"));
        }
    };
    let mut remaining = layout.data_len;
    let mut offset = layout.data_offset;
    let mut input = Vec::new();
    let mut position = 0_usize;
    let mut output = [0_u8; CHUNK_BYTES];
    let mut actual_output = 0_u64;
    let mut crc = Crc32::new();
    loop {
        if io.is_cancelled() {
            return Err(PipelineFailure::cancelled(Some(layout.id)));
        }
        if position == input.len() && remaining > 0 {
            on_read();
            let requested = remaining.min(CHUNK_BYTES as u64) as usize;
            let (buffer, count) = io
                .read_at(source.verified_file(), offset, requested)
                .map_err(|error| {
                    if io.is_cancelled() {
                        PipelineFailure::cancelled(Some(layout.id))
                    } else {
                        PipelineFailure::io(error, Some(layout.id))
                    }
                })?;
            if count == 0 {
                return Err(PipelineFailure::new(
                    ArchiveWriteFailureKind::TruncatedPayload,
                    Some(layout.id),
                    "compressed payload ended before its validated span",
                ));
            }
            if count > requested || count > buffer.len() {
                return Err(PipelineFailure::new(
                    ArchiveWriteFailureKind::SourceChanged,
                    Some(layout.id),
                    "native source read exceeded its requested span",
                ));
            }
            input.clear();
            input.extend_from_slice(&buffer[..count]);
            position = 0;
            offset = offset
                .checked_add(count as u64)
                .ok_or_else(|| quota_failure(layout.id, "source offset overflow"))?;
            remaining -= count as u64;
        }
        let (consumed, produced, ended) = if let Some(decoder) = inflater.as_mut() {
            let before_in = decoder.total_in();
            let before_out = decoder.total_out();
            let status = decoder
                .decompress(
                    &input[position..],
                    &mut output,
                    flate2::FlushDecompress::None,
                )
                .map_err(|_| {
                    PipelineFailure::new(
                        ArchiveWriteFailureKind::IntegrityMismatch,
                        Some(layout.id),
                        "invalid Deflate stream",
                    )
                })?;
            (
                (decoder.total_in() - before_in) as usize,
                (decoder.total_out() - before_out) as usize,
                status == flate2::Status::StreamEnd,
            )
        } else {
            let count = input.len() - position;
            output[..count].copy_from_slice(&input[position..]);
            (count, count, remaining == 0)
        };
        position += consumed;
        actual_output = actual_output
            .checked_add(produced as u64)
            .ok_or_else(|| quota_failure(layout.id, "decoded byte counter overflow"))?;
        if actual_output > layout.output_len {
            return Err(PipelineFailure::new(
                ArchiveWriteFailureKind::IntegrityMismatch,
                Some(layout.id),
                "decoded output exceeds the declared entry size",
            ));
        }
        if produced != 0 {
            crc.update(&output[..produced]);
            emit(&output[..produced])?;
        }
        if ended {
            if remaining != 0 || position != input.len() {
                return Err(PipelineFailure::new(
                    ArchiveWriteFailureKind::IntegrityMismatch,
                    Some(layout.id),
                    "compressed payload contains trailing bytes",
                ));
            }
            break;
        }
        if consumed == 0 && produced == 0 {
            return Err(PipelineFailure::new(
                ArchiveWriteFailureKind::TruncatedPayload,
                Some(layout.id),
                "Deflate stream ended before its end marker",
            ));
        }
    }
    if actual_output != layout.output_len {
        return Err(PipelineFailure::new(
            ArchiveWriteFailureKind::IntegrityMismatch,
            Some(layout.id),
            "decoded output differs from the declared entry size",
        ));
    }
    if crc.finish() != layout.crc32 {
        return Err(PipelineFailure::new(
            ArchiveWriteFailureKind::IntegrityMismatch,
            Some(layout.id),
            "payload CRC-32 does not match its central record",
        ));
    }
    Ok(())
}

fn abort_stage(stage: OwnedStage, mut failure: PipelineFailure) -> PipelineFailure {
    if let Err(cleanup) = stage.abort() {
        failure.message.push_str("; stage cleanup failed: ");
        failure.message.push_str(&cleanup.to_string());
        failure.kind = ArchiveWriteFailureKind::NativeIo;
        failure.cleanup_failed = cleanup.cleanup_failed();
    }
    failure
}

fn quota_failure(entry: ArchiveEntryId, message: &'static str) -> PipelineFailure {
    PipelineFailure::new(ArchiveWriteFailureKind::QuotaExceeded, Some(entry), message)
}

fn unsupported(entry: ArchiveEntryId, message: &'static str) -> PipelineFailure {
    PipelineFailure::new(
        ArchiveWriteFailureKind::UnsupportedZipLayout,
        Some(entry),
        message,
    )
}

fn set_pipeline_failure(report: &mut ArchiveWriteReport, failure: PipelineFailure) {
    report.cleanup_failed = failure.cleanup_failed;
    let cancelled = failure.kind == ArchiveWriteFailureKind::Cancelled;
    report.status = if cancelled {
        if report.published_entries.is_empty() {
            ArchiveWriteStatus::Cancelled
        } else {
            ArchiveWriteStatus::Partial
        }
    } else if report.published_entries.is_empty() {
        ArchiveWriteStatus::Failed
    } else {
        ArchiveWriteStatus::Partial
    };
    if cancelled {
        report.cancelled_entry = failure.entry;
        report.failure = None;
    } else {
        report.failed_entry = failure.entry;
        report.failure = Some((failure.kind, failure.message));
    }
}

fn set_unstarted_entries(report: &mut ArchiveWriteReport, files: &[&PlannedOutput], start: usize) {
    report.unstarted_entries.clear();
    report.unstarted_entries.extend(
        files
            .iter()
            .skip(start)
            .filter_map(|remaining| remaining.source_entry()),
    );
}

fn read_exact_at(
    source: &NativeArchiveSource,
    control: &ArchiveIoControl,
    offset: u64,
    output: &mut [u8],
    entry: Option<ArchiveEntryId>,
) -> Result<(), PipelineFailure> {
    let mut filled = 0_usize;
    while filled < output.len() {
        if control.is_cancelled() {
            return Err(PipelineFailure::cancelled(entry));
        }
        let length = (output.len() - filled).min(CHUNK_BYTES);
        let (bytes, transferred) = control
            .read_at(
                source.verified_file(),
                offset.checked_add(filled as u64).ok_or_else(|| {
                    PipelineFailure::new(
                        ArchiveWriteFailureKind::UnsupportedZipLayout,
                        entry,
                        "source read offset overflow",
                    )
                })?,
                length,
            )
            .map_err(|error| {
                if control.is_cancelled() {
                    PipelineFailure::cancelled(entry)
                } else {
                    PipelineFailure::io(error, entry)
                }
            })?;
        if transferred == 0 {
            return Err(PipelineFailure::new(
                ArchiveWriteFailureKind::TruncatedPayload,
                entry,
                "source ended before the validated ZIP structure was complete",
            ));
        }
        if transferred > length || transferred > bytes.len() {
            return Err(PipelineFailure::new(
                ArchiveWriteFailureKind::SourceChanged,
                entry,
                "native source read exceeded its requested span",
            ));
        }
        output[filled..filled + transferred].copy_from_slice(&bytes[..transferred]);
        filled += transferred;
    }
    Ok(())
}

fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
impl WriterTestGate {
    pub(super) fn new() -> (Arc<Self>, Receiver<()>, SyncSender<()>) {
        let (reached_tx, reached_rx) = mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = mpsc::sync_channel(1);
        (
            Arc::new(Self {
                reached: reached_tx,
                resume: Mutex::new(resume_rx),
            }),
            reached_rx,
            resume_tx,
        )
    }

    fn reach_and_wait(&self) {
        let _ = self.reached.try_send(());
        let _ = lock(&self.resume).recv_timeout(Duration::from_secs(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;
    use windows::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GENERIC_WRITE, HANDLE,
        INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE, OPEN_EXISTING,
        PIPE_ACCESS_INBOUND,
    };
    use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    use windows::Win32::System::Threading::CreateEventW;
    use windows::core::PCWSTR;
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    use crate::archive::native_io::{NativeIoTerminal, VerifiedOverlappedFile};
    use crate::archive::plan::{ArchiveSelection, SelectionTarget, build_extraction_plan};
    use crate::archive::{ArchiveLimits, ArchiveRevision};

    static NEXT_TEST_PIPE_ID: AtomicU64 = AtomicU64::new(1);
    const TEST_REVISION: ArchiveRevision = ArchiveRevision::new(991);
    const TEST_TIMEOUT: Duration = Duration::from_secs(10);

    struct ReleaseGateOnDrop {
        sender: Option<SyncSender<()>>,
        control: Option<ArchiveWriteJobControl>,
    }

    impl ReleaseGateOnDrop {
        fn new(sender: SyncSender<()>, control: ArchiveWriteJobControl) -> Self {
            Self {
                sender: Some(sender),
                control: Some(control),
            }
        }

        fn release(mut self) {
            if let Some(sender) = self.sender.take() {
                let _ = sender.send(());
            }
            self.control.take();
        }
    }

    impl Drop for ReleaseGateOnDrop {
        fn drop(&mut self) {
            if let Some(control) = self.control.take() {
                control.cancel();
            }
            if let Some(sender) = self.sender.take() {
                let _ = sender.send(());
            }
        }
    }

    struct ArchiveFixture {
        _temp: TempDir,
        archive: PathBuf,
        destination: PathBuf,
    }

    struct PreparedRun {
        source: NativeArchiveSource,
        catalog: ArchiveCatalog,
        plan: ExtractionPlan,
        destination: NativeArchiveDestination,
    }

    impl ArchiveFixture {
        fn new(items: &[FixtureEntry<'_>]) -> Self {
            let temp = tempfile::tempdir().expect("create archive writer fixture root");
            let archive = temp.path().join("source.zip");
            let destination = temp.path().join("destination");
            fs::create_dir(&destination).expect("create selected destination");
            write_archive(&archive, items);
            Self {
                _temp: temp,
                archive,
                destination,
            }
        }

        fn prepare(&self, limits: ArchiveLimits) -> PreparedRun {
            let mut source = NativeArchiveSource::open(&self.archive, TEST_REVISION, limits)
                .expect("admit held source archive");
            let catalog = source
                .read_catalog(&ScanCancellation::new())
                .expect("catalog the held fixture archive");
            let mut selection = ArchiveSelection::new();
            for entry in catalog.entries() {
                selection
                    .add(SelectionTarget::Entry(entry.id()), catalog.limits())
                    .expect("select fixture entry");
            }
            let plan = build_extraction_plan(&catalog, &selection, None, &ScanCancellation::new())
                .expect("build fixture output plan");
            let destination = NativeArchiveDestination::open(&self.destination)
                .expect("admit held destination root");
            PreparedRun {
                source,
                catalog,
                plan,
                destination,
            }
        }

        fn start(&self, limits: ArchiveLimits) -> ArchiveWriteJob {
            self.prepare(limits)
                .start(ArchiveIoControl::new(ScanCancellation::new()))
        }
    }

    impl PreparedRun {
        fn start(self, io: ArchiveIoControl) -> ArchiveWriteJob {
            ArchiveWriteJob::start_with_control(
                self.source,
                self.catalog,
                self.plan,
                self.destination,
                io,
            )
            .expect("spawn one owned archive writer worker")
        }

        fn start_with_gates(
            self,
            io: ArchiveIoControl,
            before_flush: Option<Arc<WriterTestGate>>,
            after_flush: Option<Arc<WriterTestGate>>,
            before_publication: Option<Arc<WriterTestGate>>,
        ) -> ArchiveWriteJob {
            ArchiveWriteJob::start_with_test_io_and_gates(
                self.source,
                self.catalog,
                self.plan,
                self.destination,
                io,
                before_flush,
                after_flush,
                before_publication,
            )
            .expect("spawn one gated archive writer worker")
        }

        fn start_with_test_hooks(
            self,
            io: ArchiveIoControl,
            hooks: WriterTestHooks,
        ) -> ArchiveWriteJob {
            ArchiveWriteJob::start_with_control_and_hooks(
                self.source,
                self.catalog,
                self.plan,
                self.destination,
                io,
                hooks,
            )
            .expect("spawn one archive writer worker with focused test hooks")
        }
    }

    enum FixtureEntry<'a> {
        File(&'a str, &'a [u8], CompressionMethod),
        Directory(&'a str),
    }

    fn write_archive(path: &Path, items: &[FixtureEntry<'_>]) {
        let file = File::create(path).expect("create ZIP fixture");
        let mut writer = ZipWriter::new(file);
        for item in items {
            match item {
                FixtureEntry::File(name, bytes, method) => {
                    writer
                        .start_file(
                            *name,
                            SimpleFileOptions::default().compression_method(*method),
                        )
                        .expect("start ZIP fixture file");
                    writer.write_all(bytes).expect("write ZIP fixture payload");
                }
                FixtureEntry::Directory(name) => {
                    writer
                        .add_directory(*name, SimpleFileOptions::default())
                        .expect("add ZIP fixture directory");
                }
            }
        }
        writer.finish().expect("finish ZIP fixture");
    }

    fn default_limits() -> ArchiveLimits {
        ArchiveLimits::default()
    }

    fn stage_count(path: &Path) -> usize {
        fs::read_dir(path)
            .expect("list selected destination")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".stage"))
            .count()
    }

    fn read_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn read_catalog_for_mutation(path: &Path) -> ArchiveCatalog {
        let mut source = NativeArchiveSource::open(path, TEST_REVISION, default_limits())
            .expect("open fixture before bounded byte mutation");
        source
            .read_catalog(&ScanCancellation::new())
            .expect("catalog fixture before bounded byte mutation")
    }

    fn local_payload_offset(bytes: &[u8], local_offset: usize) -> usize {
        let name_len = usize::from(read_u16(bytes, local_offset + 26));
        let extra_len = usize::from(read_u16(bytes, local_offset + 28));
        local_offset + 30 + name_len + extra_len
    }

    fn pending_pipe() -> io::Result<(Vec<u16>, VerifiedOverlappedFile, File)> {
        use windows::Win32::Foundation::BOOL;

        let pipe_name = format!(
            r"\\.\pipe\multi-launcher-writer-{}-{}",
            std::process::id(),
            NEXT_TEST_PIPE_ID.fetch_add(1, Ordering::Relaxed)
        );
        let name: Vec<u16> = std::ffi::OsStr::new(&pipe_name)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let server_handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_INBOUND | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                4096,
                4096,
                0,
                None,
            )
        };
        if server_handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // Immediately transfer the raw fixture handle into a Rust owner so
        // every later setup error closes the pipe instance.
        let server = unsafe { File::from_raw_handle(server_handle.0 as RawHandle) };
        let server_handle = HANDLE(server.as_raw_handle() as _);
        let client_handle = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                HANDLE::default(),
            )
        }
        .map_err(|error| io::Error::from_raw_os_error(win32_code(&error) as i32))?;
        let client = unsafe { File::from_raw_handle(client_handle.0 as RawHandle) };
        let event_handle = unsafe { CreateEventW(None, BOOL(1), BOOL(0), PCWSTR::null()) }
            .map_err(|error| io::Error::from_raw_os_error(win32_code(&error) as i32))?;
        let event = unsafe { File::from_raw_handle(event_handle.0 as RawHandle) };
        let mut overlapped = Box::pin(OVERLAPPED {
            hEvent: HANDLE(event.as_raw_handle() as _),
            ..Default::default()
        });
        let overlapped_ptr = std::pin::Pin::as_mut(&mut overlapped).get_mut() as *mut OVERLAPPED;
        let connected = unsafe { ConnectNamedPipe(server_handle, Some(overlapped_ptr)) };
        if let Err(error) = connected {
            if win32_code(&error) == ERROR_PIPE_CONNECTED.0 {
                // The client connected before the server issued ConnectNamedPipe.
            } else if win32_code(&error) == ERROR_IO_PENDING.0 {
                let mut transferred = 0_u32;
                unsafe {
                    GetOverlappedResult(server_handle, overlapped_ptr, &mut transferred, BOOL(1))
                }
                .map_err(|error| io::Error::from_raw_os_error(win32_code(&error) as i32))?;
            } else {
                return Err(io::Error::from_raw_os_error(win32_code(&error) as i32));
            }
        }
        drop(overlapped);
        let verified = VerifiedOverlappedFile::verify(server)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "pipe mode not overlapped"))?;
        Ok((name, verified, client))
    }

    fn win32_code(error: &windows::core::Error) -> u32 {
        let code = error.code().0 as u32;
        if code & 0xffff_0000 == 0x8007_0000 {
            code & 0xffff
        } else {
            code
        }
    }

    fn assert_pipe_retired(name: &[u16]) {
        let reopened = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                HANDLE::default(),
            )
        };
        match reopened {
            Err(error) => assert_eq!(
                win32_code(&error),
                ERROR_FILE_NOT_FOUND.0,
                "pipe retirement must be reported as ERROR_FILE_NOT_FOUND"
            ),
            Ok(handle) => {
                // SAFETY: immediately own and close an unexpectedly reopened
                // handle before reporting the fixture failure.
                drop(unsafe { File::from_raw_handle(handle.0 as RawHandle) });
                panic!("test pipe instance remained open after join");
            }
        }
    }

    #[test]
    fn writer_publishes_multiple_empty_and_nested_stored_entries() {
        let fixture = ArchiveFixture::new(&[
            FixtureEntry::Directory("root/"),
            FixtureEntry::File(
                "root/sub/data.bin",
                b"nested bytes",
                CompressionMethod::Stored,
            ),
            FixtureEntry::File("empty.bin", b"", CompressionMethod::Stored),
        ]);
        let job = fixture.start(default_limits());
        let report = job.join();
        assert_eq!(report.status, ArchiveWriteStatus::Completed);
        assert_eq!(report.published_entries.len(), 2);
        assert_eq!(report.actual_bytes_written, 12);
        assert_eq!(
            fs::read(fixture.destination.join("root/sub/data.bin")).unwrap(),
            b"nested bytes"
        );
        assert_eq!(
            fs::read(fixture.destination.join("empty.bin")).unwrap(),
            b""
        );
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn writer_streams_a_payload_across_bounded_native_chunks() {
        let payload = (0..(CHUNK_BYTES * 2 + 137))
            .map(|index| (index.wrapping_mul(37) & 0xff) as u8)
            .collect::<Vec<_>>();
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "multi-chunk.bin",
            &payload,
            CompressionMethod::Stored,
        )]);

        let report = fixture.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Completed);
        assert_eq!(report.actual_bytes_written, payload.len() as u64);
        assert_eq!(
            fs::read(fixture.destination.join("multi-chunk.bin")).unwrap(),
            payload
        );
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn corrupt_second_payload_keeps_first_publication_and_cleans_failed_stage() {
        let fixture = ArchiveFixture::new(&[
            FixtureEntry::File("a-first.bin", b"first", CompressionMethod::Stored),
            FixtureEntry::File("b-second.bin", b"second", CompressionMethod::Stored),
        ]);
        let catalog = read_catalog_for_mutation(&fixture.archive);
        let second = catalog
            .entries()
            .iter()
            .find(|entry| entry.name().as_utf8() == Some("b-second.bin"))
            .unwrap();
        let mut bytes = fs::read(&fixture.archive).unwrap();
        let payload = local_payload_offset(&bytes, second.local_header_offset() as usize);
        bytes[payload] ^= 0x01;
        fs::write(&fixture.archive, bytes).unwrap();

        let report = fixture.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Partial);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::IntegrityMismatch)
        );
        assert_eq!(report.failed_entry, Some(second.id()));
        assert_eq!(report.published_entries.len(), 1);
        assert_eq!(
            fs::read(fixture.destination.join("a-first.bin")).unwrap(),
            b"first"
        );
        assert!(!fixture.destination.join("b-second.bin").exists());
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn physical_validation_refuses_truncation_and_overlap_before_writes() {
        let truncation = ArchiveFixture::new(&[FixtureEntry::File(
            "truncated.bin",
            b"abc",
            CompressionMethod::Stored,
        )]);
        let catalog = read_catalog_for_mutation(&truncation.archive);
        let entry = &catalog.entries()[0];
        let mut bytes = fs::read(&truncation.archive).unwrap();
        let payload = local_payload_offset(&bytes, entry.local_header_offset() as usize);
        let central = catalog.source().central_directory_offset as usize;
        bytes.remove(payload + 2);
        let eocd = bytes
            .windows(4)
            .rposition(|window| window == 0x0605_4b50_u32.to_le_bytes())
            .unwrap();
        write_u32(&mut bytes, eocd + 16, (central - 1) as u32);
        fs::write(&truncation.archive, bytes).unwrap();
        let report = truncation.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::UnsupportedZipLayout)
        );
        assert!(!truncation.destination.join("truncated.bin").exists());

        let overlap = ArchiveFixture::new(&[
            FixtureEntry::File("a.bin", b"a", CompressionMethod::Stored),
            FixtureEntry::File("b.bin", b"b", CompressionMethod::Stored),
        ]);
        let catalog = read_catalog_for_mutation(&overlap.archive);
        let first_local = catalog.entries()[0].local_header_offset() as u32;
        let second = &catalog.entries()[1];
        let mut bytes = fs::read(&overlap.archive).unwrap();
        write_u32(
            &mut bytes,
            second.central_header_offset() as usize + 42,
            first_local,
        );
        fs::write(&overlap.archive, bytes).unwrap();
        let report = overlap.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::UnsupportedZipLayout)
        );
        assert_eq!(fs::read_dir(&overlap.destination).unwrap().count(), 0);
    }

    #[test]
    fn writer_extracts_mixed_deflated_multichunk_and_empty_entries() {
        let mut random = 0x1234_5678_u32;
        let payload: Vec<_> = (0..CHUNK_BYTES * 3 + 137)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect();
        let fixture = ArchiveFixture::new(&[
            FixtureEntry::Directory("folder/"),
            FixtureEntry::File("deflated-dir/", b"", CompressionMethod::Deflated),
            FixtureEntry::File("folder/stored", b"stored", CompressionMethod::Stored),
            FixtureEntry::File("folder/large", &payload, CompressionMethod::Deflated),
            FixtureEntry::File("empty", b"", CompressionMethod::Deflated),
        ]);
        let prepared = fixture.prepare(default_limits());
        assert!(
            prepared
                .catalog
                .entries()
                .iter()
                .find(|entry| entry.name().as_utf8() == Some("folder/large"))
                .unwrap()
                .compressed_size()
                > CHUNK_BYTES as u64
        );
        validate_plan_for_preview(
            &prepared.source,
            &prepared.catalog,
            &prepared.plan,
            &ArchiveIoControl::new(ScanCancellation::new()),
        )
        .expect("preview admits validated Deflate entries");
        let report = prepared
            .start(ArchiveIoControl::new(ScanCancellation::new()))
            .join();
        assert_eq!(
            report.status,
            ArchiveWriteStatus::Completed,
            "{:?}",
            report.failure
        );
        assert_eq!(
            fs::read(fixture.destination.join("folder/large")).unwrap(),
            payload
        );
        assert_eq!(
            fs::read(fixture.destination.join("folder/stored")).unwrap(),
            b"stored"
        );
        assert_eq!(fs::read(fixture.destination.join("empty")).unwrap(), b"");
        assert_eq!(report.actual_bytes_written, payload.len() as u64 + 6);
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn deflate_descriptor_and_zip64_fixture_extracts() {
        let fixture = ArchiveFixture::new(&[]);
        let mut bytes = Vec::new();
        {
            let mut writer = ZipWriter::new_stream(&mut bytes);
            writer
                .start_file(
                    "streamed",
                    SimpleFileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .large_file(true),
                )
                .unwrap();
            writer.write_all(b"descriptor and ZIP64 deflate").unwrap();
            writer.finish().unwrap();
        }
        let mut conflicting = bytes.clone();
        let extra_start = 30 + read_u16(&conflicting, 26) as usize;
        conflicting[extra_start + 4] = 1;
        let invalid = ArchiveFixture::new(&[]);
        fs::write(&invalid.archive, conflicting).unwrap();
        let invalid_report = invalid.start(default_limits()).join();
        assert_eq!(invalid_report.status, ArchiveWriteStatus::Failed);
        assert_eq!(
            invalid_report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::UnsupportedZipLayout)
        );
        assert_eq!(fs::read_dir(&invalid.destination).unwrap().count(), 0);
        fs::write(&fixture.archive, bytes).unwrap();
        let report = fixture.start(default_limits()).join();
        assert_eq!(
            report.status,
            ArchiveWriteStatus::Completed,
            "{:?}",
            report.failure
        );
        assert_eq!(
            fs::read(fixture.destination.join("streamed")).unwrap(),
            b"descriptor and ZIP64 deflate"
        );
    }

    #[test]
    fn deflate_rejects_declared_size_and_crc_mismatch() {
        for mismatch in ["short", "long", "crc"] {
            let fixture = ArchiveFixture::new(&[FixtureEntry::File(
                "data",
                b"deflated bytes",
                CompressionMethod::Deflated,
            )]);
            let catalog = read_catalog_for_mutation(&fixture.archive);
            let entry = &catalog.entries()[0];
            let mut bytes = fs::read(&fixture.archive).unwrap();
            let local = entry.local_header_offset() as usize;
            let central = entry.central_header_offset() as usize;
            if mismatch == "crc" {
                write_u32(&mut bytes, local + 14, 0x12345678);
                write_u32(&mut bytes, central + 16, 0x12345678);
            } else {
                let size = if mismatch == "short" { 3 } else { 25 };
                write_u32(&mut bytes, local + 22, size);
                write_u32(&mut bytes, central + 24, size);
            }
            fs::write(&fixture.archive, bytes).unwrap();
            let report = fixture.start(default_limits()).join();
            assert_eq!(report.status, ArchiveWriteStatus::Failed, "{mismatch}");
            assert_eq!(
                report.failure.as_ref().map(|(kind, _)| *kind),
                Some(ArchiveWriteFailureKind::IntegrityMismatch),
                "{mismatch}"
            );
            assert!(!fixture.destination.join("data").exists());
            assert_eq!(stage_count(&fixture.destination), 0);
        }
    }

    #[test]
    fn corrupt_deflated_directory_is_rejected_before_destination_mutation() {
        let fixture = ArchiveFixture::new(&[
            FixtureEntry::File("bad-dir/", b"", CompressionMethod::Deflated),
            FixtureEntry::File("output", b"x", CompressionMethod::Stored),
        ]);
        let catalog = read_catalog_for_mutation(&fixture.archive);
        let mut bytes = fs::read(&fixture.archive).unwrap();
        let offset =
            local_payload_offset(&bytes, catalog.entries()[0].local_header_offset() as usize);
        bytes[offset] = 0xff;
        fs::write(&fixture.archive, bytes).unwrap();
        let report = fixture.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 0);
    }

    #[test]
    fn truncated_deflate_stream_is_not_published() {
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "data",
            b"a compressed stream to truncate",
            CompressionMethod::Deflated,
        )]);
        let catalog = read_catalog_for_mutation(&fixture.archive);
        let entry = &catalog.entries()[0];
        let mut bytes = fs::read(&fixture.archive).unwrap();
        let local = entry.local_header_offset() as usize;
        let payload = local_payload_offset(&bytes, local);
        let length = entry.compressed_size() as usize;
        bytes.remove(payload + length - 1);
        let central = entry.central_header_offset() as usize - 1;
        write_u32(&mut bytes, local + 18, (length - 1) as u32);
        write_u32(&mut bytes, central + 20, (length - 1) as u32);
        let eocd = bytes
            .windows(4)
            .rposition(|window| window == 0x0605_4b50_u32.to_le_bytes())
            .unwrap();
        write_u32(&mut bytes, eocd + 16, central as u32);
        fs::write(&fixture.archive, bytes).unwrap();
        let report = fixture.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert!(matches!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(
                ArchiveWriteFailureKind::TruncatedPayload
                    | ArchiveWriteFailureKind::IntegrityMismatch
            )
        ));
        assert!(!fixture.destination.join("data").exists());
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn corrupt_deflate_keeps_earlier_publication_and_cleans_stage() {
        let fixture = ArchiveFixture::new(&[
            FixtureEntry::File("a", b"first", CompressionMethod::Stored),
            FixtureEntry::File("b", b"deflated content", CompressionMethod::Deflated),
        ]);
        let catalog = read_catalog_for_mutation(&fixture.archive);
        let mut bytes = fs::read(&fixture.archive).unwrap();
        let offset =
            local_payload_offset(&bytes, catalog.entries()[1].local_header_offset() as usize);
        bytes[offset] = 0xff;
        fs::write(&fixture.archive, bytes).unwrap();
        let report = fixture.start(default_limits()).join();
        assert_eq!(
            report.status,
            ArchiveWriteStatus::Partial,
            "failure={:?}, published={:?}",
            report.failure,
            report.published_entries
        );
        assert!(matches!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(
                ArchiveWriteFailureKind::IntegrityMismatch
                    | ArchiveWriteFailureKind::TruncatedPayload
            )
        ));
        assert_eq!(fs::read(fixture.destination.join("a")).unwrap(), b"first");
        assert!(!fixture.destination.join("b").exists());
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn cancellation_between_deflate_chunks_aborts_owned_stage() {
        let payload = (0..CHUNK_BYTES * 2 + 17)
            .map(|i| ((i * 37 + i / 257) & 255) as u8)
            .collect::<Vec<_>>();
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "large.bin",
            &payload,
            CompressionMethod::Deflated,
        )]);
        let prepared = fixture.prepare(default_limits());
        let (gate, reached, release) = WriterTestGate::new();
        let job = prepared.start_with_test_hooks(
            ArchiveIoControl::new(ScanCancellation::new()),
            WriterTestHooks {
                after_payload_chunk: Some(gate),
                ..WriterTestHooks::default()
            },
        );
        let guard = ReleaseGateOnDrop::new(release, job.control());
        reached
            .recv_timeout(TEST_TIMEOUT)
            .expect("first decoded chunk reached stage");
        job.cancel();
        guard.release();
        let report = job.join();
        assert_eq!(report.status, ArchiveWriteStatus::Cancelled);
        assert!(report.actual_bytes_written > 0);
        assert!(!fixture.destination.join("large.bin").exists());
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn zip64_extra_layout_is_exact_and_unknown_fields_fail_closed() {
        let id = ArchiveEntryId::new(TEST_REVISION, 0);
        let mut valid = Vec::new();
        valid.extend_from_slice(&ZIP64_EXTRA_ID.to_le_bytes());
        valid.extend_from_slice(&16_u16.to_le_bytes());
        valid.extend_from_slice(&7_u64.to_le_bytes());
        valid.extend_from_slice(&7_u64.to_le_bytes());
        let values = parse_zip64_extra(&valid, true, true, false, false, id)
            .expect("accept exact ZIP64 sizes in field order");
        assert_eq!(values.uncompressed, Some(7));
        assert_eq!(values.compressed, Some(7));
        assert_eq!(values.local_offset, None);

        let mut overlong = valid.clone();
        overlong.extend_from_slice(&[0x34, 0x12, 0, 0]);
        assert!(parse_zip64_extra(&overlong, true, true, false, false, id).is_err());
        assert!(parse_zip64_extra(&[0x34, 0x12, 0, 0], false, false, false, false, id).is_err());
    }

    #[test]
    fn local_and_central_crc_must_match_before_stage_creation() {
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "mismatch.bin",
            b"local crc remains unchanged",
            CompressionMethod::Stored,
        )]);
        let catalog = read_catalog_for_mutation(&fixture.archive);
        let entry = &catalog.entries()[0];
        let mut bytes = fs::read(&fixture.archive).unwrap();
        let central = entry.central_header_offset() as usize;
        let central_crc = read_u32(&bytes, central + 16);
        write_u32(&mut bytes, central + 16, central_crc ^ 1);
        fs::write(&fixture.archive, bytes).unwrap();

        let report = fixture.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::SourceChanged)
        );
        assert_eq!(report.failed_entry, Some(entry.id()));
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 0);
    }

    #[test]
    fn unexplained_bytes_before_the_central_directory_are_refused() {
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "gap.bin",
            b"payload",
            CompressionMethod::Stored,
        )]);
        let catalog = read_catalog_for_mutation(&fixture.archive);
        let central = catalog.source().central_directory_offset as usize;
        let mut bytes = fs::read(&fixture.archive).unwrap();
        bytes.splice(central..central, [0x41, 0x42, 0x43, 0x44]);
        let eocd = bytes
            .windows(4)
            .rposition(|window| window == 0x0605_4b50_u32.to_le_bytes())
            .unwrap();
        write_u32(&mut bytes, eocd + 16, (central + 4) as u32);
        fs::write(&fixture.archive, bytes).unwrap();

        let report = fixture.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::UnsupportedZipLayout)
        );
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 0);
    }

    #[test]
    fn omitted_entry_span_cannot_overlap_a_selected_physical_record() {
        let fixture = ArchiveFixture::new(&[
            FixtureEntry::File("omitted.bin", b"a", CompressionMethod::Stored),
            FixtureEntry::File("selected.bin", b"b", CompressionMethod::Stored),
        ]);
        let original = read_catalog_for_mutation(&fixture.archive);
        let omitted = &original.entries()[0];
        let selected = original.entries()[1].id();
        let mut bytes = fs::read(&fixture.archive).unwrap();
        let first_local = omitted.local_header_offset() as usize;
        let first_data = local_payload_offset(&bytes, first_local);
        let second_local = original.entries()[1].local_header_offset() as usize;
        let claimed_size = u32::try_from(second_local - first_data + 1).unwrap();
        write_u32(&mut bytes, first_local + 18, claimed_size);
        write_u32(&mut bytes, first_local + 22, claimed_size);
        let first_central = omitted.central_header_offset() as usize;
        write_u32(&mut bytes, first_central + 20, claimed_size);
        write_u32(&mut bytes, first_central + 24, claimed_size);
        fs::write(&fixture.archive, bytes).unwrap();

        let mut source =
            NativeArchiveSource::open(&fixture.archive, TEST_REVISION, default_limits())
                .expect("open archive with an overlapping omitted local record");
        let catalog = source
            .read_catalog(&ScanCancellation::new())
            .expect("catalog central metadata before physical validation");
        let mut selection = ArchiveSelection::new();
        selection
            .add(SelectionTarget::Entry(selected), catalog.limits())
            .expect("select only the later entry");
        let plan = build_extraction_plan(&catalog, &selection, None, &ScanCancellation::new())
            .expect("build plan for the selected later entry");
        let destination = NativeArchiveDestination::open(&fixture.destination)
            .expect("admit selected destination root");
        let report =
            ArchiveWriteJob::start(source, catalog, plan, destination, ScanCancellation::new())
                .expect("start bounded writer");
        let report = report.join();

        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::UnsupportedZipLayout)
        );
        assert_eq!(report.failed_entry, Some(omitted.id()));
        assert!(report.published_entries.is_empty());
        assert_eq!(report.unstarted_entries.as_slice(), &[selected]);
        assert!(!fixture.destination.join("selected.bin").exists());
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 0);
    }

    #[test]
    fn held_source_must_match_plan_catalog_before_destination_mutation() {
        let source_fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "same.bin",
            b"source bytes",
            CompressionMethod::Stored,
        )]);
        let planned_fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "same.bin",
            b"plan bytes!!",
            CompressionMethod::Stored,
        )]);
        let source_run = source_fixture.prepare(default_limits());
        let planned_run = planned_fixture.prepare(default_limits());
        assert_eq!(source_run.catalog.source(), planned_run.catalog.source());
        assert_ne!(source_run.catalog.entries(), planned_run.catalog.entries());
        let expected_unstarted = planned_run
            .plan
            .files()
            .filter_map(|output| output.source_entry())
            .collect::<Vec<_>>();

        let run = PreparedRun {
            source: source_run.source,
            catalog: planned_run.catalog,
            plan: planned_run.plan,
            destination: planned_run.destination,
        };
        let report = run
            .start(ArchiveIoControl::new(ScanCancellation::new()))
            .join();
        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::SourceChanged)
        );
        assert!(report.published_entries.is_empty());
        assert_eq!(report.unstarted_entries, expected_unstarted);
        assert_eq!(
            fs::read_dir(&planned_fixture.destination).unwrap().count(),
            0
        );
    }

    #[test]
    fn actual_entry_and_aggregate_quotas_allow_exact_and_stop_cross_entry_write() {
        let exact = ArchiveFixture::new(&[
            FixtureEntry::File("a.bin", b"abc", CompressionMethod::Stored),
            FixtureEntry::File("b.bin", b"de", CompressionMethod::Deflated),
        ]);
        let mut limits = default_limits();
        limits.max_actual_entry_output_bytes = 3;
        limits.max_actual_total_output_bytes = 5;
        let exact_report = exact.start(limits).join();
        assert_eq!(exact_report.status, ArchiveWriteStatus::Completed);
        assert_eq!(exact_report.actual_bytes_written, 5);

        let crossing = ArchiveFixture::new(&[
            FixtureEntry::File("a.bin", b"abc", CompressionMethod::Stored),
            FixtureEntry::File("b.bin", b"de", CompressionMethod::Deflated),
        ]);
        limits.max_actual_total_output_bytes = 4;
        let report = crossing.start(limits).join();
        assert_eq!(report.status, ArchiveWriteStatus::Partial);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::QuotaExceeded)
        );
        assert_eq!(report.actual_bytes_written, 3);
        assert_eq!(
            fs::read(crossing.destination.join("a.bin")).unwrap(),
            b"abc"
        );
        assert!(!crossing.destination.join("b.bin").exists());
        assert_eq!(stage_count(&crossing.destination), 0);
    }

    #[test]
    fn competitor_created_after_plan_is_never_replaced() {
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "w",
            b"archive data",
            CompressionMethod::Stored,
        )]);
        let prepared = fixture.prepare(default_limits());
        fs::write(fixture.destination.join("w"), b"foreign winner")
            .expect("add competitor after plan construction");
        let report = prepared
            .start(ArchiveIoControl::new(ScanCancellation::new()))
            .join();
        assert_eq!(report.status, ArchiveWriteStatus::Failed);
        assert_eq!(
            report.failure.as_ref().map(|(kind, _)| *kind),
            Some(ArchiveWriteFailureKind::NativeIo)
        );
        assert!(
            report.failure.as_ref().unwrap().1.contains("3221225525"),
            "{:?}",
            report.failure
        );
        assert_eq!(
            fs::read(fixture.destination.join("w")).unwrap(),
            b"foreign winner"
        );
        assert_eq!(stage_count(&fixture.destination), 0);
        assert!(
            !report.cleanup_failed,
            "successful cleanup of the incomplete stage is not a cleanup failure"
        );
    }

    #[test]
    fn cancellation_around_synchronous_flush_removes_stage_before_publication() {
        for gate_position in [true, false] {
            let fixture = ArchiveFixture::new(&[FixtureEntry::File(
                "flush.bin",
                b"must not publish",
                CompressionMethod::Deflated,
            )]);
            let (gate, reached, release) = WriterTestGate::new();
            let prepared = fixture.prepare(default_limits());
            let expected_id = prepared.catalog.entries()[0].id();
            let job = prepared.start_with_gates(
                ArchiveIoControl::new(ScanCancellation::new()),
                gate_position.then(|| Arc::clone(&gate)),
                (!gate_position).then_some(gate),
                None,
            );
            reached
                .recv_timeout(TEST_TIMEOUT)
                .expect("writer reached the requested flush boundary");
            job.cancel();
            release.send(()).expect("release finalization gate");
            let report = job.join();
            assert_eq!(report.status, ArchiveWriteStatus::Cancelled);
            assert_eq!(report.cancelled_entry, Some(expected_id));
            assert!(!fixture.destination.join("flush.bin").exists());
            assert_eq!(stage_count(&fixture.destination), 0);
        }
    }

    #[test]
    fn cancellation_after_publication_admission_reports_completed_output() {
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "admitted.bin",
            b"publication already admitted",
            CompressionMethod::Stored,
        )]);
        let (gate, reached, release) = WriterTestGate::new();
        let prepared = fixture.prepare(default_limits());
        let expected_id = prepared.catalog.entries()[0].id();
        let job = prepared.start_with_gates(
            ArchiveIoControl::new(ScanCancellation::new()),
            None,
            None,
            Some(gate),
        );
        reached
            .recv_timeout(TEST_TIMEOUT)
            .expect("writer admitted final no-replace publication");
        assert_eq!(job.progress().phase, ArchiveWritePhase::Publishing);
        job.cancel();
        release.send(()).expect("release admitted publication");

        let report = job.join();
        assert_eq!(report.status, ArchiveWriteStatus::Completed);
        assert_eq!(report.published_entries, [expected_id]);
        assert_eq!(report.phase, ArchiveWritePhase::Retired);
        assert!(report.failure.is_none());
        assert_eq!(
            fs::read(fixture.destination.join("admitted.bin")).unwrap(),
            b"publication already admitted"
        );
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn writer_retirement_waits_for_join_and_ignores_late_cancel() {
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "retirement.bin",
            b"finished before join",
            CompressionMethod::Stored,
        )]);
        let job = fixture.start(default_limits());
        let control = job.control();
        let deadline = std::time::Instant::now() + TEST_TIMEOUT;
        while !job
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            assert!(
                std::time::Instant::now() < deadline,
                "writer did not finish before the bounded pre-join check"
            );
            thread::yield_now();
        }

        assert_ne!(control.progress().phase, ArchiveWritePhase::Retired);
        let report = job.join();
        assert_eq!(report.status, ArchiveWriteStatus::Completed);
        assert_eq!(report.phase, ArchiveWritePhase::Retired);
        assert_eq!(control.progress().phase, ArchiveWritePhase::Retired);

        control.cancel();
        assert_eq!(control.progress().phase, ArchiveWritePhase::Retired);
        assert_eq!(
            fs::read(fixture.destination.join("retirement.bin")).unwrap(),
            b"finished before join"
        );
    }

    #[test]
    fn cancellation_between_entries_reports_partial_and_keeps_prior_file() {
        let fixture = ArchiveFixture::new(&[
            FixtureEntry::File("first.bin", b"published", CompressionMethod::Stored),
            FixtureEntry::File("second.bin", b"not started", CompressionMethod::Stored),
        ]);
        let prepared = fixture.prepare(default_limits());
        let first_id = prepared.catalog.entries()[0].id();
        let second_id = prepared.catalog.entries()[1].id();
        let (gate, reached, release) = WriterTestGate::new();
        let job = prepared.start_with_test_hooks(
            ArchiveIoControl::new(ScanCancellation::new()),
            WriterTestHooks {
                before_next_entry: Some(gate),
                ..WriterTestHooks::default()
            },
        );
        let release_guard = ReleaseGateOnDrop::new(release, job.control());
        reached
            .recv_timeout(TEST_TIMEOUT)
            .expect("first file is published before cancellation gate");
        assert!(fixture.destination.join("first.bin").exists());

        job.cancel();
        release_guard.release();
        let report = job.join();
        assert_eq!(report.status, ArchiveWriteStatus::Partial);
        assert_eq!(report.published_entries, [first_id]);
        assert_eq!(report.cancelled_entry, Some(second_id));
        assert_eq!(report.unstarted_entries, Vec::<ArchiveEntryId>::new());
        assert!(report.failure.is_none());
        assert_eq!(
            fs::read(fixture.destination.join("first.bin")).unwrap(),
            b"published"
        );
        assert!(!fixture.destination.join("second.bin").exists());
        assert_eq!(stage_count(&fixture.destination), 0);
    }

    #[test]
    fn production_job_cancels_pending_catalog_read_and_retires_before_restart() {
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "pending.bin",
            b"source payload",
            CompressionMethod::Stored,
        )]);
        let prepared = fixture.prepare(default_limits());
        let (pipe_name, pipe_file, client) =
            pending_pipe().expect("create overlapped pipe fixture");
        let io = ArchiveIoControl::with_test_io_override(ScanCancellation::new(), pipe_file);
        let job = prepared.start(io.clone());
        assert!(io.wait_for_active_request_pending(TEST_TIMEOUT));
        job.cancel();
        job.cancel();
        let report = job.join();
        assert_eq!(report.status, ArchiveWriteStatus::Cancelled);
        let (request_id, terminal, resources_released) = io
            .last_test_request()
            .expect("observe production job request completion");
        assert_ne!(request_id, 0);
        assert!(matches!(
            terminal,
            NativeIoTerminal::Failed {
                operation: "GetOverlappedResult(ReadFile)",
                hresult,
                win32_code: Some(995),
            } if hresult == (0x8007_0000_u32 | 995) as i32
        ));
        assert!(resources_released);
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 0);
        drop(io);
        drop(client);
        assert_pipe_retired(&pipe_name);

        let report = fixture.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Completed);
        assert_eq!(
            fs::read(fixture.destination.join("pending.bin")).unwrap(),
            b"source payload"
        );
    }

    #[test]
    fn production_job_drop_cancels_pending_payload_after_partial_stage_and_restarts() {
        let payload = vec![0x5a; CHUNK_BYTES * 2 + 17];
        let fixture = ArchiveFixture::new(&[FixtureEntry::File(
            "pending-payload.bin",
            &payload,
            CompressionMethod::Stored,
        )]);
        let prepared = fixture.prepare(default_limits());
        let expected_id = prepared.catalog.entries()[0].id();
        let (pipe_name, pipe_file, client) =
            pending_pipe().expect("create overlapped pipe fixture");
        let io = ArchiveIoControl::new(ScanCancellation::new());
        let (gate, reached, release) = WriterTestGate::new();
        let job = prepared.start_with_test_hooks(
            io.clone(),
            WriterTestHooks {
                after_payload_chunk: Some(gate),
                ..WriterTestHooks::default()
            },
        );
        let release_guard = ReleaseGateOnDrop::new(release, job.control());
        reached
            .recv_timeout(TEST_TIMEOUT)
            .expect("production writer has committed one payload chunk to its owned stage");
        let staged = fs::read_dir(&fixture.destination)
            .expect("inspect task-owned destination stage")
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().ends_with(".stage"))
            .expect("partial payload stage exists before the pending read");
        let stage_probe = File::open(staged.path()).expect("open read-only stage probe");
        let staged_len = stage_probe
            .metadata()
            .expect("inspect partial stage length")
            .len();
        drop(stage_probe);
        assert!(
            staged_len > 0 && staged_len < payload.len() as u64,
            "held stage length {staged_len} should be nonzero and below payload length {}",
            payload.len()
        );

        io.replace_test_io_override(pipe_file);
        release_guard.release();
        assert!(io.wait_for_active_request_pending(TEST_TIMEOUT));
        let job_control = job.control();
        drop(job);
        let progress = job_control.progress();
        assert_eq!(progress.phase, ArchiveWritePhase::Retired);
        assert_eq!(progress.published_count, 0);
        assert!(progress.actual_bytes_written > 0);
        let (request_id, terminal, resources_released) = io
            .last_test_request()
            .expect("observe the production payload request completion");
        assert_ne!(request_id, 0);
        assert!(matches!(
            terminal,
            NativeIoTerminal::Failed {
                operation: "GetOverlappedResult(ReadFile)",
                hresult,
                win32_code: Some(995),
            } if hresult == (0x8007_0000_u32 | 995) as i32
        ));
        assert!(resources_released);
        assert_eq!(expected_id, progress.current_entry.unwrap());
        assert!(!fixture.destination.join("pending-payload.bin").exists());
        assert_eq!(stage_count(&fixture.destination), 0);
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 0);

        drop(job_control);
        drop(io);
        drop(client);
        assert_pipe_retired(&pipe_name);

        let report = fixture.start(default_limits()).join();
        assert_eq!(report.status, ArchiveWriteStatus::Completed);
        assert_eq!(report.published_entries, [expected_id]);
        assert_eq!(
            fs::read(fixture.destination.join("pending-payload.bin")).unwrap(),
            payload
        );
    }
}
