//! Bounded, deterministic selection and destination-conflict planning.
//!
//! This module never creates directories, staging files, or outputs. A
//! successful plan is lexical metadata only; execution must revalidate source
//! identity and destination handles before any write.
use std::{collections::HashSet, io::ErrorKind};

use super::model::{
    ArchiveCatalog, ArchiveEntry, ArchiveEntryId, ArchiveEntryName, ArchiveFormat, ArchiveLimits,
    ArchiveRevision, ArchiveSource, EntryCompression, EntryKind, EntryProblems,
    NameEncodingProblem, PathCopyError, SafeRelativePath, UnsafeEntryReason, UnsafePathReason,
    ValidatedRelativePath, raw_name_is_descendant, windows_relative_cmp,
};
use crate::file_operations::ScanCancellation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionTarget {
    Entry(ArchiveEntryId),
    FolderSubtree(ArchiveEntryId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionAddError {
    LimitExceeded,
    Allocation,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArchiveSelection {
    targets: Vec<SelectionTarget>,
}

impl ArchiveSelection {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn targets(&self) -> &[SelectionTarget] {
        &self.targets
    }

    pub fn add(
        &mut self,
        target: SelectionTarget,
        limits: &ArchiveLimits,
    ) -> Result<(), SelectionAddError> {
        if self.targets.len() >= limits.max_selection_entries {
            return Err(SelectionAddError::LimitExceeded);
        }
        self.targets
            .try_reserve(1)
            .map_err(|_| SelectionAddError::Allocation)?;
        self.targets.push(target);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationOccupancy {
    Vacant,
    Directory,
    File,
    Other,
    ReparsePoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanLimit {
    SelectionEntries,
    Work,
    Nodes,
    PathBytes,
    OccupancyProbes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanIssueReason {
    EmptySelection,
    StaleRevision,
    UnknownEntryOrdinal,
    FolderSelectionRequiresDirectory,
    InvalidNameEncoding(NameEncodingProblem),
    UnsafePath(UnsafePathReason),
    UnsafeEntryKind(UnsafeEntryReason),
    EncryptedEntry,
    UnsupportedFlags(u16),
    UnsupportedCompression(u16),
    InconsistentStoredSize,
    DirectoryHasPayload,
    EntryDeclaredOutputTooLarge,
    ExpansionRatioExceeded,
    TotalDeclaredOutputTooLarge,
    DuplicateFileOutputPath,
    FileDirectoryOutputCollision,
    FileIsOutputAncestor,
    ExistingFileAtOutput,
    ExistingDirectoryAtFileOutput,
    ExistingFileAncestor,
    ExistingUnsupportedObject,
    ExistingReparsePoint,
    OccupancyProbeFailed(ErrorKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanIssue {
    pub entry: Option<ArchiveEntryId>,
    pub related_entry: Option<ArchiveEntryId>,
    pub reason: PlanIssueReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanReport {
    revision: ArchiveRevision,
    issues: Vec<PlanIssue>,
    omitted_issues: usize,
}

impl PlanReport {
    fn try_new(revision: ArchiveRevision, max: usize) -> Result<Self, PlanBuildError> {
        let mut issues = Vec::new();
        issues
            .try_reserve_exact(max)
            .map_err(|_| PlanBuildError::Allocation)?;
        Ok(Self {
            revision,
            issues,
            omitted_issues: 0,
        })
    }

    fn record(&mut self, issue: PlanIssue, max: usize) {
        if self.issues.len() < max {
            self.issues.push(issue);
        } else {
            self.omitted_issues = self.omitted_issues.saturating_add(1);
        }
    }

    pub const fn revision(&self) -> ArchiveRevision {
        self.revision
    }
    pub fn issues(&self) -> &[PlanIssue] {
        &self.issues
    }
    pub const fn omitted_issues(&self) -> usize {
        self.omitted_issues
    }
    pub const fn is_truncated(&self) -> bool {
        self.omitted_issues != 0
    }
    fn has_issues(&self) -> bool {
        !self.issues.is_empty() || self.omitted_issues != 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanBuildError {
    Cancelled,
    Allocation,
    LimitExceeded(PlanLimit),
    Rejected(PlanReport),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapturedOutputPolicy {
    pub no_overwrite: bool,
    pub max_actual_entry_output_bytes: u64,
    pub max_actual_total_output_bytes: u64,
    pub max_expansion_ratio: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedOutput {
    path: ValidatedRelativePath,
    kind: EntryKind,
    source_entry: Option<ArchiveEntryId>,
    compressed_size: u64,
    declared_output_size: u64,
}

impl PlannedOutput {
    pub fn path(&self) -> &ValidatedRelativePath {
        &self.path
    }
    pub const fn kind(&self) -> EntryKind {
        self.kind
    }
    pub const fn source_entry(&self) -> Option<ArchiveEntryId> {
        self.source_entry
    }
    pub const fn compressed_size(&self) -> u64 {
        self.compressed_size
    }
    pub const fn declared_output_size(&self) -> u64 {
        self.declared_output_size
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractionPlan {
    source: ArchiveSource,
    outputs: Vec<PlannedOutput>,
    declared_total_output_bytes: u64,
    policy: CapturedOutputPolicy,
}

impl ExtractionPlan {
    pub const fn source(&self) -> ArchiveSource {
        self.source
    }
    pub fn outputs(&self) -> &[PlannedOutput] {
        &self.outputs
    }
    pub fn files(&self) -> impl Iterator<Item = &PlannedOutput> {
        self.outputs.iter().filter(|v| v.kind == EntryKind::File)
    }
    pub fn directories(&self) -> impl Iterator<Item = &PlannedOutput> {
        self.outputs
            .iter()
            .filter(|v| v.kind == EntryKind::Directory)
    }
    pub const fn declared_total_output_bytes(&self) -> u64 {
        self.declared_total_output_bytes
    }
    pub const fn policy(&self) -> CapturedOutputPolicy {
        self.policy
    }
}

struct Work {
    used: usize,
    limit: usize,
}
impl Work {
    fn new(limit: usize) -> Self {
        Self { used: 0, limit }
    }
    fn add(&mut self, n: usize) -> Result<(), PlanBuildError> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or(PlanBuildError::LimitExceeded(PlanLimit::Work))?;
        if self.used > self.limit {
            Err(PlanBuildError::LimitExceeded(PlanLimit::Work))
        } else {
            Ok(())
        }
    }
    fn step(&mut self) -> Result<(), PlanBuildError> {
        self.add(1)
    }
}

struct Prepared<'a> {
    entry: &'a ArchiveEntry,
    path: SafeRelativePath<'a>,
}

fn select_id(
    id: ArchiveEntryId,
    ids: &mut Vec<ArchiveEntryId>,
    seen: &mut HashSet<ArchiveEntryId>,
    limits: &ArchiveLimits,
) -> Result<(), PlanBuildError> {
    if seen.contains(&id) {
        return Ok(());
    }
    if ids.len() >= limits.max_selection_entries {
        return Err(PlanBuildError::LimitExceeded(PlanLimit::SelectionEntries));
    }
    seen.try_reserve(1)
        .map_err(|_| PlanBuildError::Allocation)?;
    ids.try_reserve(1).map_err(|_| PlanBuildError::Allocation)?;
    seen.insert(id);
    ids.push(id);
    Ok(())
}

/// Resolve revision-bound identities and produce a pure output plan. An
/// optional occupancy callback must be read-only; it is called only within the
/// configured probe bound.
pub fn build_extraction_plan(
    catalog: &ArchiveCatalog,
    selection: &ArchiveSelection,
    occupancy_probe: Option<
        &mut dyn FnMut(&ValidatedRelativePath) -> Result<DestinationOccupancy, ErrorKind>,
    >,
    cancellation: &ScanCancellation,
) -> Result<ExtractionPlan, PlanBuildError> {
    build_extraction_plan_with_finalization(
        catalog,
        selection,
        occupancy_probe,
        cancellation,
        || {},
    )
}

fn build_extraction_plan_with_finalization(
    catalog: &ArchiveCatalog,
    selection: &ArchiveSelection,
    mut occupancy_probe: Option<
        &mut dyn FnMut(&ValidatedRelativePath) -> Result<DestinationOccupancy, ErrorKind>,
    >,
    cancellation: &ScanCancellation,
    before_success: impl FnOnce(),
) -> Result<ExtractionPlan, PlanBuildError> {
    let source = catalog.source();
    let limits = catalog.limits();
    let mut report = PlanReport::try_new(source.revision, limits.max_diagnostics)?;
    let mut work = Work::new(limits.max_plan_work);

    if selection.targets.is_empty() {
        report.record(
            PlanIssue {
                entry: None,
                related_entry: None,
                reason: PlanIssueReason::EmptySelection,
            },
            limits.max_diagnostics,
        );
        return Err(PlanBuildError::Rejected(report));
    }

    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for target in selection.targets.iter().copied() {
        work.step()?;
        check_cancel(cancellation)?;
        let id = match target {
            SelectionTarget::Entry(id) | SelectionTarget::FolderSubtree(id) => id,
        };
        let Some(root) = catalog.entry(id) else {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: if id.revision() != source.revision {
                        PlanIssueReason::StaleRevision
                    } else {
                        PlanIssueReason::UnknownEntryOrdinal
                    },
                },
                limits.max_diagnostics,
            );
            continue;
        };
        match target {
            SelectionTarget::Entry(id) => select_id(id, &mut ids, &mut seen, limits)?,
            SelectionTarget::FolderSubtree(id) => {
                if root.kind() != EntryKind::Directory {
                    report.record(
                        PlanIssue {
                            entry: Some(id),
                            related_entry: None,
                            reason: PlanIssueReason::FolderSelectionRequiresDirectory,
                        },
                        limits.max_diagnostics,
                    );
                    continue;
                }
                let Some(parent) = root.name().as_utf8() else {
                    report.record(
                        PlanIssue {
                            entry: Some(id),
                            related_entry: None,
                            reason: PlanIssueReason::InvalidNameEncoding(
                                root.problems()
                                    .name_encoding
                                    .unwrap_or(NameEncodingProblem::InvalidUtf8),
                            ),
                        },
                        limits.max_diagnostics,
                    );
                    continue;
                };
                select_id(id, &mut ids, &mut seen, limits)?;
                let parent = parent.trim_end_matches(['/', '\\']);
                for entry in catalog.entries() {
                    work.step()?;
                    check_cancel(cancellation)?;
                    if entry.id() != id && raw_name_is_descendant(entry.name().raw_bytes(), parent)
                    {
                        select_id(entry.id(), &mut ids, &mut seen, limits)?;
                    }
                }
            }
        }
    }

    if ids.is_empty() && !report.has_issues() {
        report.record(
            PlanIssue {
                entry: None,
                related_entry: None,
                reason: PlanIssueReason::EmptySelection,
            },
            limits.max_diagnostics,
        );
    }

    let mut prepared = Vec::new();
    prepared
        .try_reserve_exact(ids.len())
        .map_err(|_| PlanBuildError::Allocation)?;
    let mut declared_total = 0u64;
    for id in ids.iter().copied() {
        work.step()?;
        check_cancel(cancellation)?;
        let Some(entry) = catalog.entry(id) else {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::UnknownEntryOrdinal,
                },
                limits.max_diagnostics,
            );
            continue;
        };
        let problems = entry.problems();
        let mut valid = true;
        if let Some(encoding) = problems.name_encoding {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::InvalidNameEncoding(encoding),
                },
                limits.max_diagnostics,
            );
            valid = false;
        }
        if let Some(reason) = problems.unsafe_kind {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::UnsafeEntryKind(reason),
                },
                limits.max_diagnostics,
            );
            valid = false;
        }
        if problems.encrypted {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::EncryptedEntry,
                },
                limits.max_diagnostics,
            );
            valid = false;
        }
        if problems.unsupported_flags != 0 {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::UnsupportedFlags(problems.unsupported_flags),
                },
                limits.max_diagnostics,
            );
            valid = false;
        }
        if problems.inconsistent_stored_size {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::InconsistentStoredSize,
                },
                limits.max_diagnostics,
            );
            valid = false;
        }
        if let EntryCompression::Unsupported(code) = entry.compression() {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::UnsupportedCompression(code),
                },
                limits.max_diagnostics,
            );
            valid = false;
        }
        if let Some(reason) = problems.unsafe_path {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::UnsafePath(reason),
                },
                limits.max_diagnostics,
            );
            valid = false;
        }

        let safe_path = if problems.name_encoding.is_some() {
            None
        } else {
            match entry.safe_relative_path(limits) {
                Ok(path) => Some(path),
                Err(reason) => {
                    if problems.unsafe_path != Some(reason) {
                        report.record(
                            PlanIssue {
                                entry: Some(id),
                                related_entry: None,
                                reason: PlanIssueReason::UnsafePath(reason),
                            },
                            limits.max_diagnostics,
                        );
                    }
                    valid = false;
                    None
                }
            }
        };

        if entry.kind() == EntryKind::Directory
            && (entry.compressed_size() != 0 || entry.uncompressed_size() != 0)
        {
            report.record(
                PlanIssue {
                    entry: Some(id),
                    related_entry: None,
                    reason: PlanIssueReason::DirectoryHasPayload,
                },
                limits.max_diagnostics,
            );
            valid = false;
        }
        if entry.kind() == EntryKind::File {
            if entry.uncompressed_size() > limits.max_entry_declared_output_bytes {
                report.record(
                    PlanIssue {
                        entry: Some(id),
                        related_entry: None,
                        reason: PlanIssueReason::EntryDeclaredOutputTooLarge,
                    },
                    limits.max_diagnostics,
                );
                valid = false;
            }
            let ratio_exceeded = entry.uncompressed_size() > 0
                && (entry.compressed_size() == 0
                    || entry
                        .compressed_size()
                        .checked_mul(limits.max_expansion_ratio)
                        .is_some_and(|maximum| entry.uncompressed_size() > maximum));
            if ratio_exceeded {
                report.record(
                    PlanIssue {
                        entry: Some(id),
                        related_entry: None,
                        reason: PlanIssueReason::ExpansionRatioExceeded,
                    },
                    limits.max_diagnostics,
                );
                valid = false;
            }
            match declared_total.checked_add(entry.uncompressed_size()) {
                Some(total) if total <= limits.max_total_declared_output_bytes => {
                    declared_total = total
                }
                _ => {
                    report.record(
                        PlanIssue {
                            entry: Some(id),
                            related_entry: None,
                            reason: PlanIssueReason::TotalDeclaredOutputTooLarge,
                        },
                        limits.max_diagnostics,
                    );
                    valid = false;
                }
            }
        }
        if valid {
            if let Some(path) = safe_path {
                prepared.push(Prepared { entry, path });
            }
        }
    }
    if report.has_issues() {
        return Err(PlanBuildError::Rejected(report));
    }

    // Preflight all path copies and node slots before allocating output strings.
    let mut node_bound = 0usize;
    let mut path_bytes_bound = 0usize;
    for item in &prepared {
        check_cancel(cancellation)?;
        node_bound = node_bound
            .checked_add(item.path.component_count())
            .ok_or(PlanBuildError::LimitExceeded(PlanLimit::Nodes))?;
        if node_bound > limits.max_plan_nodes {
            return Err(PlanBuildError::LimitExceeded(PlanLimit::Nodes));
        }
        path_bytes_bound = path_bytes_bound
            .checked_add(item.path.as_str().len())
            .ok_or(PlanBuildError::LimitExceeded(PlanLimit::PathBytes))?;
        for (index, separator) in item.path.as_str().char_indices() {
            work.step()?;
            if matches!(separator, '/' | '\\') && index > 0 {
                path_bytes_bound = path_bytes_bound
                    .checked_add(index)
                    .ok_or(PlanBuildError::LimitExceeded(PlanLimit::PathBytes))?;
            }
        }
        if path_bytes_bound > limits.max_plan_path_bytes {
            return Err(PlanBuildError::LimitExceeded(PlanLimit::PathBytes));
        }
    }

    let mut outputs = Vec::new();
    outputs
        .try_reserve_exact(node_bound)
        .map_err(|_| PlanBuildError::Allocation)?;
    if outputs.capacity() > limits.max_plan_nodes {
        return Err(PlanBuildError::LimitExceeded(PlanLimit::Nodes));
    }
    let mut path_capacity = 0usize;
    for item in prepared {
        check_cancel(cancellation)?;
        let value = item.path.as_str();
        for (index, separator) in value.char_indices() {
            work.step()?;
            if matches!(separator, '/' | '\\') && index > 0 {
                let parent = ValidatedRelativePath::from_parent_prefix(&value[..index], limits)
                    .map_err(map_path_copy_error)?;
                push_output(
                    &mut outputs,
                    &mut path_capacity,
                    limits,
                    PlannedOutput {
                        path: parent,
                        kind: EntryKind::Directory,
                        source_entry: None,
                        compressed_size: 0,
                        declared_output_size: 0,
                    },
                )?;
            }
        }
        let path =
            ValidatedRelativePath::from_safe(item.path, limits).map_err(map_path_copy_error)?;
        push_output(
            &mut outputs,
            &mut path_capacity,
            limits,
            PlannedOutput {
                path,
                kind: item.entry.kind(),
                source_entry: Some(item.entry.id()),
                compressed_size: item.entry.compressed_size(),
                declared_output_size: if item.entry.kind() == EntryKind::File {
                    item.entry.uncompressed_size()
                } else {
                    0
                },
            },
        )?;
    }

    work.add(sort_work(outputs.len())?)?;
    outputs.sort_by(|a, b| {
        windows_relative_cmp(a.path.as_str(), b.path.as_str())
            .then_with(|| a.path.as_str().cmp(b.path.as_str()))
            .then_with(|| output_kind_rank(a.kind).cmp(&output_kind_rank(b.kind)))
            .then_with(|| a.source_entry.cmp(&b.source_entry))
    });
    outputs.dedup_by(|a, b| {
        if windows_relative_cmp(a.path.as_str(), b.path.as_str()) != std::cmp::Ordering::Equal {
            return false;
        }
        match (a.kind, b.kind) {
            (EntryKind::Directory, EntryKind::Directory) => true,
            (EntryKind::File, EntryKind::File) => {
                report.record(
                    PlanIssue {
                        entry: a.source_entry,
                        related_entry: b.source_entry,
                        reason: PlanIssueReason::DuplicateFileOutputPath,
                    },
                    limits.max_diagnostics,
                );
                true
            }
            _ => {
                report.record(
                    PlanIssue {
                        entry: a.source_entry,
                        related_entry: b.source_entry,
                        reason: PlanIssueReason::FileDirectoryOutputCollision,
                    },
                    limits.max_diagnostics,
                );
                true
            }
        }
    });
    if report.has_issues() {
        return Err(PlanBuildError::Rejected(report));
    }

    // In component-wise sorted order, children of any path are contiguous.
    for i in 0..outputs.len() {
        if outputs[i].kind != EntryKind::File {
            continue;
        }
        for j in (i + 1)..outputs.len() {
            work.step()?;
            if !strict_ancestor(outputs[i].path.as_str(), outputs[j].path.as_str()) {
                break;
            }
            report.record(
                PlanIssue {
                    entry: outputs[i].source_entry,
                    related_entry: outputs[j].source_entry,
                    reason: PlanIssueReason::FileIsOutputAncestor,
                },
                limits.max_diagnostics,
            );
        }
    }
    if report.has_issues() {
        return Err(PlanBuildError::Rejected(report));
    }

    if let Some(probe) = occupancy_probe.as_mut() {
        for (index, output) in outputs.iter().enumerate() {
            check_cancel(cancellation)?;
            if index >= limits.max_occupancy_probes {
                return Err(PlanBuildError::LimitExceeded(PlanLimit::OccupancyProbes));
            }
            let occupancy_result = (**probe)(output.path());
            check_cancel(cancellation)?;
            let occupancy = match occupancy_result {
                Ok(value) => value,
                Err(kind) => {
                    report.record(
                        PlanIssue {
                            entry: output.source_entry,
                            related_entry: None,
                            reason: PlanIssueReason::OccupancyProbeFailed(kind),
                        },
                        limits.max_diagnostics,
                    );
                    return Err(PlanBuildError::Rejected(report));
                }
            };
            let reason = match (output.kind, occupancy) {
                (_, DestinationOccupancy::Vacant)
                | (EntryKind::Directory, DestinationOccupancy::Directory) => None,
                (_, DestinationOccupancy::ReparsePoint) => {
                    Some(PlanIssueReason::ExistingReparsePoint)
                }
                (EntryKind::File, DestinationOccupancy::File) => {
                    Some(PlanIssueReason::ExistingFileAtOutput)
                }
                (EntryKind::Directory, DestinationOccupancy::File) => {
                    Some(PlanIssueReason::ExistingFileAncestor)
                }
                (EntryKind::File, DestinationOccupancy::Directory) => {
                    Some(PlanIssueReason::ExistingDirectoryAtFileOutput)
                }
                (_, DestinationOccupancy::Other) => {
                    Some(PlanIssueReason::ExistingUnsupportedObject)
                }
            };
            if let Some(reason) = reason {
                report.record(
                    PlanIssue {
                        entry: output.source_entry,
                        related_entry: None,
                        reason,
                    },
                    limits.max_diagnostics,
                );
            }
        }
    }
    if report.has_issues() {
        return Err(PlanBuildError::Rejected(report));
    }

    let plan = ExtractionPlan {
        source,
        outputs,
        declared_total_output_bytes: declared_total,
        policy: CapturedOutputPolicy {
            no_overwrite: true,
            max_actual_entry_output_bytes: limits.max_actual_entry_output_bytes,
            max_actual_total_output_bytes: limits.max_actual_total_output_bytes,
            max_expansion_ratio: limits.max_expansion_ratio,
        },
    };
    before_success();
    check_cancel(cancellation)?;
    Ok(plan)
}

fn push_output(
    outputs: &mut Vec<PlannedOutput>,
    path_capacity: &mut usize,
    limits: &ArchiveLimits,
    output: PlannedOutput,
) -> Result<(), PlanBuildError> {
    if outputs.len() >= limits.max_plan_nodes {
        return Err(PlanBuildError::LimitExceeded(PlanLimit::Nodes));
    }
    *path_capacity = (*path_capacity)
        .checked_add(output.path.retained_capacity())
        .ok_or(PlanBuildError::LimitExceeded(PlanLimit::PathBytes))?;
    if *path_capacity > limits.max_plan_path_bytes {
        return Err(PlanBuildError::LimitExceeded(PlanLimit::PathBytes));
    }
    outputs.push(output);
    Ok(())
}

fn map_path_copy_error(error: PathCopyError) -> PlanBuildError {
    match error {
        PathCopyError::Allocation => PlanBuildError::Allocation,
        PathCopyError::Limit | PathCopyError::Unsafe(_) => {
            PlanBuildError::LimitExceeded(PlanLimit::PathBytes)
        }
    }
}

fn sort_work(count: usize) -> Result<usize, PlanBuildError> {
    if count < 2 {
        return Ok(count);
    }
    let log = usize::BITS as usize - count.leading_zeros() as usize;
    count
        .checked_mul(log)
        .ok_or(PlanBuildError::LimitExceeded(PlanLimit::Work))
}

const fn output_kind_rank(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Directory => 0,
        EntryKind::File => 1,
    }
}

fn strict_ancestor(parent: &str, child: &str) -> bool {
    let mut a = parent.split(['/', '\\']);
    let mut b = child.split(['/', '\\']);
    loop {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) if windows_relative_cmp(x, y) == std::cmp::Ordering::Equal => {}
            (Some(_), _) => return false,
            (None, Some(_)) => return true,
            (None, None) => return false,
        }
    }
}

fn check_cancel(token: &ScanCancellation) -> Result<(), PlanBuildError> {
    if token.is_cancelled() {
        Err(PlanBuildError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        revision: ArchiveRevision,
        ordinal: u64,
        name: ArchiveEntryName,
        kind: EntryKind,
        compression: EntryCompression,
        compressed: u64,
        expanded: u64,
        problems: EntryProblems,
    ) -> ArchiveEntry {
        ArchiveEntry {
            id: ArchiveEntryId::new(revision, ordinal),
            name,
            kind,
            compression,
            flags: 0,
            crc32: 0,
            compressed_size: compressed,
            uncompressed_size: expanded,
            local_header_offset: ordinal * 64,
            central_header_offset: ordinal * 64,
            version_made_by: 20,
            external_attributes: 0,
            problems,
        }
    }

    fn file(
        rev: ArchiveRevision,
        ordinal: u64,
        name: &str,
        method: EntryCompression,
        c: u64,
        u: u64,
    ) -> ArchiveEntry {
        let unsafe_path =
            SafeRelativePath::validate(name, EntryKind::File, &ArchiveLimits::default()).err();
        entry(
            rev,
            ordinal,
            ArchiveEntryName::Utf8(name.into()),
            EntryKind::File,
            method,
            c,
            u,
            EntryProblems {
                unsafe_path,
                ..EntryProblems::default()
            },
        )
    }

    fn catalog(entries: Vec<ArchiveEntry>, limits: ArchiveLimits) -> ArchiveCatalog {
        let revision = entries
            .first()
            .map_or(ArchiveRevision::new(17), |e| e.id().revision());
        ArchiveCatalog::new(
            ArchiveSource {
                revision,
                archive_len: 1024,
                central_directory_offset: 512,
                central_directory_size: 0,
                central_directory_end: 512,
                entry_count: entries.len() as u64,
                format: ArchiveFormat::Zip32,
            },
            limits,
            entries,
            0,
        )
    }

    fn selection(catalog: &ArchiveCatalog, targets: &[SelectionTarget]) -> ArchiveSelection {
        let mut selection = ArchiveSelection::new();
        for target in targets {
            selection.add(*target, catalog.limits()).unwrap();
        }
        selection
    }

    fn build(
        catalog: &ArchiveCatalog,
        targets: &[SelectionTarget],
    ) -> Result<ExtractionPlan, PlanBuildError> {
        build_extraction_plan(
            catalog,
            &selection(catalog, targets),
            None,
            &ScanCancellation::new(),
        )
    }

    #[test]
    fn physical_duplicate_names_keep_distinct_ids_and_conflict_only_when_both_selected() {
        let rev = ArchiveRevision::new(17);
        let catalog = catalog(
            vec![
                file(rev, 0, "same.txt", EntryCompression::Stored, 2, 2),
                file(rev, 1, "same.txt", EntryCompression::Stored, 3, 3),
            ],
            ArchiveLimits::default(),
        );
        let one = build(
            &catalog,
            &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 1))],
        )
        .unwrap();
        assert_eq!(
            one.files().next().unwrap().source_entry(),
            Some(ArchiveEntryId::new(rev, 1))
        );
        let error = build(
            &catalog,
            &[
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 0)),
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 1)),
            ],
        )
        .unwrap_err();
        let PlanBuildError::Rejected(report) = error else {
            panic!("duplicate outputs must be rejected");
        };
        assert!(
            report
                .issues()
                .iter()
                .any(|i| i.reason == PlanIssueReason::DuplicateFileOutputPath)
        );
    }

    #[test]
    fn folder_subtree_expansion_refuses_problem_entries_without_hiding_them() {
        let rev = ArchiveRevision::new(18);
        let mut encrypted = file(rev, 2, "folder/secret.bin", EntryCompression::Stored, 1, 1);
        encrypted.problems.encrypted = true;
        let mut reserved = file(rev, 3, "folder/CON.txt", EntryCompression::Stored, 1, 1);
        reserved.problems.unsafe_path = Some(UnsafePathReason::ReservedDeviceName);
        let root = entry(
            rev,
            0,
            ArchiveEntryName::Utf8("folder/".into()),
            EntryKind::Directory,
            EntryCompression::Stored,
            0,
            0,
            EntryProblems::default(),
        );
        let entries = vec![
            root,
            file(rev, 1, "folder/ok.txt", EntryCompression::Deflated, 3, 7),
            encrypted,
            reserved,
        ];
        let catalog = catalog(entries, ArchiveLimits::default());
        assert_eq!(catalog.entries().len(), 4);
        let error = build(
            &catalog,
            &[SelectionTarget::FolderSubtree(ArchiveEntryId::new(rev, 0))],
        )
        .unwrap_err();
        let PlanBuildError::Rejected(report) = error else {
            panic!("unsafe descendant must block subtree");
        };
        assert!(
            report
                .issues()
                .iter()
                .any(|i| i.entry == Some(ArchiveEntryId::new(rev, 2))
                    && i.reason == PlanIssueReason::EncryptedEntry)
        );
        assert!(
            report
                .issues()
                .iter()
                .any(|i| i.entry == Some(ArchiveEntryId::new(rev, 3))
                    && i.reason
                        == PlanIssueReason::UnsafePath(UnsafePathReason::ReservedDeviceName))
        );
    }

    #[test]
    fn windows_case_separator_and_file_ancestor_collisions_are_rejected() {
        let rev = ArchiveRevision::new(19);
        let aliases = catalog(
            vec![
                file(rev, 0, "Root/file.txt", EntryCompression::Stored, 2, 2),
                file(rev, 1, "root\\FILE.TXT", EntryCompression::Stored, 2, 2),
            ],
            ArchiveLimits::default(),
        );
        let error = build(
            &aliases,
            &[
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 0)),
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 1)),
            ],
        )
        .unwrap_err();
        let PlanBuildError::Rejected(report) = error else {
            panic!("Windows path aliases must conflict");
        };
        assert!(
            report
                .issues()
                .iter()
                .any(|i| i.reason == PlanIssueReason::DuplicateFileOutputPath)
        );

        let ancestor = catalog(
            vec![
                file(rev, 0, "node", EntryCompression::Stored, 1, 1),
                file(rev, 1, "node/child", EntryCompression::Stored, 1, 1),
            ],
            ArchiveLimits::default(),
        );
        let error = build(
            &ancestor,
            &[
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 0)),
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 1)),
            ],
        )
        .unwrap_err();
        let PlanBuildError::Rejected(report) = error else {
            panic!("file ancestor must conflict");
        };
        assert!(report.issues().iter().any(|i| matches!(
            i.reason,
            PlanIssueReason::FileDirectoryOutputCollision | PlanIssueReason::FileIsOutputAncestor
        )));
    }

    #[test]
    fn stale_identity_unsupported_method_and_declared_ratio_have_reasons() {
        let rev = ArchiveRevision::new(20);
        let mut limits = ArchiveLimits::default();
        limits.max_expansion_ratio = 4;
        let unsupported = file(rev, 0, "odd.bin", EntryCompression::Unsupported(99), 1, 1);
        let large_ratio = file(rev, 1, "large.bin", EntryCompression::Deflated, 10, 50);
        let mut unsupported_flags = file(rev, 2, "flags.bin", EntryCompression::Stored, 1, 1);
        unsupported_flags.problems.unsupported_flags = 1 << 5;
        let catalog = catalog(vec![unsupported, large_ratio, unsupported_flags], limits);
        let stale = build(
            &catalog,
            &[SelectionTarget::Entry(ArchiveEntryId::new(
                ArchiveRevision::new(21),
                0,
            ))],
        )
        .unwrap_err();
        let PlanBuildError::Rejected(stale_report) = stale else {
            panic!("stale id should refuse");
        };
        assert!(
            stale_report
                .issues()
                .iter()
                .any(|i| i.reason == PlanIssueReason::StaleRevision)
        );
        let unsupported_flags = build(
            &catalog,
            &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 2))],
        )
        .unwrap_err();
        let PlanBuildError::Rejected(flags_report) = unsupported_flags else {
            panic!("unsupported flags should refuse");
        };
        assert!(
            flags_report
                .issues()
                .iter()
                .any(|i| i.reason == PlanIssueReason::UnsupportedFlags(1 << 5))
        );
        let unsupported = build(
            &catalog,
            &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 0))],
        )
        .unwrap_err();
        let PlanBuildError::Rejected(unsupported_report) = unsupported else {
            panic!("unsupported method should refuse");
        };
        assert!(
            unsupported_report
                .issues()
                .iter()
                .any(|i| i.reason == PlanIssueReason::UnsupportedCompression(99))
        );
        let ratio = build(
            &catalog,
            &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 1))],
        )
        .unwrap_err();
        let PlanBuildError::Rejected(ratio_report) = ratio else {
            panic!("ratio estimate should refuse");
        };
        assert!(
            ratio_report
                .issues()
                .iter()
                .any(|i| i.reason == PlanIssueReason::ExpansionRatioExceeded)
        );
    }

    #[test]
    fn read_only_occupancy_probe_blocks_existing_file_and_is_bounded() {
        let rev = ArchiveRevision::new(22);
        let catalog = catalog(
            vec![file(rev, 0, "out/file.txt", EntryCompression::Stored, 4, 4)],
            ArchiveLimits::default(),
        );
        let selection = selection(
            &catalog,
            &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 0))],
        );
        let mut calls = 0;
        let mut probe = |path: &ValidatedRelativePath| {
            calls += 1;
            Ok(if path.as_str().ends_with("file.txt") {
                DestinationOccupancy::File
            } else {
                DestinationOccupancy::Vacant
            })
        };
        let error = build_extraction_plan(
            &catalog,
            &selection,
            Some(&mut probe),
            &ScanCancellation::new(),
        )
        .unwrap_err();
        assert_eq!(
            calls, 2,
            "one implicit directory and one file were queried; no mutation API exists"
        );
        let PlanBuildError::Rejected(report) = error else {
            panic!("occupied file must refuse");
        };
        assert!(
            report
                .issues()
                .iter()
                .any(|i| i.reason == PlanIssueReason::ExistingFileAtOutput)
        );
    }

    #[test]
    fn plan_is_deterministic_coalesces_directories_and_carries_future_guards() {
        let rev = ArchiveRevision::new(23);
        let catalog = catalog(
            vec![
                file(rev, 0, "b/two.txt", EntryCompression::Stored, 4, 4),
                file(rev, 1, "a/one.txt", EntryCompression::Stored, 3, 3),
            ],
            ArchiveLimits::default(),
        );
        let a = build(
            &catalog,
            &[
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 0)),
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 1)),
            ],
        )
        .unwrap();
        let b = build(
            &catalog,
            &[
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 1)),
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 0)),
            ],
        )
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a.files().map(|o| o.path().as_str()).collect::<Vec<_>>(),
            ["a/one.txt", "b/two.txt"]
        );
        assert_eq!(a.directories().count(), 2);
        assert_eq!(a.declared_total_output_bytes(), 7);
        assert!(a.policy().no_overwrite);
        assert_eq!(
            a.policy().max_actual_total_output_bytes,
            catalog.limits().max_actual_total_output_bytes
        );
    }

    #[test]
    fn plan_limits_and_cancellation_return_no_partial_plan() {
        let rev = ArchiveRevision::new(24);
        let mut limits = ArchiveLimits::default();
        limits.max_plan_nodes = 2;
        let limited_catalog = catalog(
            vec![file(rev, 0, "one/two/file", EntryCompression::Stored, 1, 1)],
            limits,
        );
        assert_eq!(
            build(
                &limited_catalog,
                &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 0))]
            ),
            Err(PlanBuildError::LimitExceeded(PlanLimit::Nodes))
        );

        let cancellation_catalog = catalog(
            vec![file(rev, 0, "a.txt", EntryCompression::Stored, 1, 1)],
            ArchiveLimits::default(),
        );
        let selected = selection(
            &cancellation_catalog,
            &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 0))],
        );
        let token = ScanCancellation::new();
        token.cancel();
        assert_eq!(
            build_extraction_plan(&cancellation_catalog, &selected, None, &token),
            Err(PlanBuildError::Cancelled)
        );
    }

    #[test]
    fn case_aliased_implicit_directory_spelling_is_stable_across_selection_order() {
        let rev = ArchiveRevision::new(24);
        let archive = catalog(
            vec![
                file(rev, 0, "case/first.txt", EntryCompression::Stored, 1, 1),
                file(rev, 1, "CASE/second.txt", EntryCompression::Stored, 1, 1),
            ],
            ArchiveLimits::default(),
        );
        let forward = build(
            &archive,
            &[
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 0)),
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 1)),
            ],
        )
        .unwrap();
        let reversed = build(
            &archive,
            &[
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 1)),
                SelectionTarget::Entry(ArchiveEntryId::new(rev, 0)),
            ],
        )
        .unwrap();

        assert_eq!(forward, reversed);
        assert_eq!(
            forward
                .directories()
                .map(|output| output.path().as_str())
                .collect::<Vec<_>>(),
            ["CASE"]
        );
    }

    #[test]
    fn cancellation_after_final_occupancy_callback_never_returns_a_plan() {
        let rev = ArchiveRevision::new(25);
        let archive = catalog(
            vec![file(rev, 0, "one/file.txt", EntryCompression::Stored, 1, 1)],
            ArchiveLimits::default(),
        );
        let selected = selection(
            &archive,
            &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 0))],
        );
        let token = ScanCancellation::new();
        let mut calls = 0;
        let mut probe = |path: &ValidatedRelativePath| {
            calls += 1;
            if path.as_str() == "one/file.txt" {
                token.cancel();
            }
            Ok(DestinationOccupancy::Vacant)
        };

        assert_eq!(
            build_extraction_plan(&archive, &selected, Some(&mut probe), &token),
            Err(PlanBuildError::Cancelled)
        );
        assert_eq!(calls, 2, "cancellation occurs in the last probe callback");
    }

    #[test]
    fn cancellation_after_plan_construction_prevents_success_without_occupancy_probe() {
        let rev = ArchiveRevision::new(26);
        let archive = catalog(
            vec![file(rev, 0, "ready.txt", EntryCompression::Stored, 1, 1)],
            ArchiveLimits::default(),
        );
        let selected = selection(
            &archive,
            &[SelectionTarget::Entry(ArchiveEntryId::new(rev, 0))],
        );
        let token = ScanCancellation::new();
        let mut finalized = false;

        let result =
            build_extraction_plan_with_finalization(&archive, &selected, None, &token, || {
                finalized = true;
                token.cancel();
            });

        assert!(
            finalized,
            "the hook runs after a complete plan was constructed"
        );
        assert!(token.is_cancelled());
        assert_eq!(result, Err(PlanBuildError::Cancelled));
    }
}
