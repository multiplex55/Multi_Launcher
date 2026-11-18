//! Sequential execution of approved copy and duplicate entries.

use std::io;
use std::path::Path;

use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_BASIC_INFO};

use crate::archive::transfer_native::{
    TransferDirectoryCreateFailure, TransferDirectoryCreateOutcome,
};
use crate::file_operations::set_replace_basic_info;

use super::copy::{
    copy_to_created_parent, copy_to_replacement_reporting, copy_to_vacancy_reporting,
    require_no_directory_streams,
};
use super::journal::TransferJournal;
use super::{
    AdmissionError, ApprovedTarget, ApprovedTransferEntry, ApprovedTransferPlan, CopiedFile,
    CopyArtifact, CopyArtifactState, CopyControl, CopyError, CopyFailure, CopyRecoveryArtifact,
    DirectoryAccess, EntryKind, EntrySnapshot, HeldParent, ParentSnapshot, SourceNodeId,
    TargetOccupancy, TargetSnapshot, TransferAction, checked_final_path, copy_to_vacancy,
    directory_stamp, inspect_target, observe_entry_metadata,
};

#[derive(Clone, Copy, Debug)]
pub enum ExecutionProgress {
    Preparing(SourceNodeId),
    Copying {
        entry: SourceNodeId,
        staged_bytes: u64,
    },
    DirectoryCreated(SourceNodeId),
    Finalizing(SourceNodeId),
    Verifying {
        entry: SourceNodeId,
        verification_requested: bool,
    },
    FileCompleted {
        entry: SourceNodeId,
        bytes: u64,
    },
}

#[derive(Debug)]
pub enum DirectoryTimes {
    Pending,
    Preserved,
    Incomplete,
    Failed(CopyFailure),
}

#[derive(Debug)]
pub struct DirectoryCopy {
    pub output: CopyArtifact,
    pub times: DirectoryTimes,
    snapshot: ParentSnapshot,
}

#[derive(Debug)]
pub enum ExecutionState {
    Copied(CopiedFile),
    CreatedDirectory(DirectoryCopy),
    MergedDirectory(ParentSnapshot),
    Skipped,
    BlockedByParent(SourceNodeId),
    BlockedByReceipt,
    Failed(CopyError),
    Cancelled(Option<CopyError>),
}

impl ExecutionState {
    fn admitted_directory(&self) -> bool {
        matches!(self, Self::CreatedDirectory(_) | Self::MergedDirectory(_))
    }
}

#[derive(Debug)]
pub struct ExecutionRow {
    pub entry: SourceNodeId,
    pub state: ExecutionState,
}

#[derive(Debug)]
pub struct CopyExecution {
    pub rows: Vec<ExecutionRow>,
    pub cancelled: bool,
}

/// Apply the approved Copy or Duplicate in parent-first order. Every
/// publication uses an exact namespace and a currently held native parent.
pub fn execute_copy_plan(
    plan: &ApprovedTransferPlan,
    control: &CopyControl,
    progress: impl FnMut(ExecutionProgress),
) -> Result<CopyExecution, CopyFailure> {
    execute_copy_with_hooks(plan, control, progress, |_| {}, None)
}

fn execute_copy_with_hooks(
    plan: &ApprovedTransferPlan,
    control: &CopyControl,
    mut progress: impl FnMut(ExecutionProgress),
    mut before_directory_create: impl FnMut(&Path),
    mut journal: Option<&mut dyn TransferJournal>,
) -> Result<CopyExecution, CopyFailure> {
    if matches!(plan.draft().action, TransferAction::MoveTo(_))
        || (journal.is_none()
            && plan
                .entries()
                .iter()
                .any(|entry| matches!(entry.destination(), ApprovedTarget::Replace(_))))
    {
        return Err(CopyFailure::UnsupportedPlan);
    }

    let mut rows: Vec<_> = plan
        .source()
        .entries()
        .iter()
        .map(|entry| ExecutionRow {
            entry: entry.id(),
            state: ExecutionState::Skipped,
        })
        .collect();
    let mut journal_failed = false;
    for entry in plan.entries() {
        let id = entry.id();
        let parent = plan.source().get(id).and_then(|source| source.parent());
        let state = if journal_failed {
            ExecutionState::BlockedByReceipt
        } else if control.is_cancelled() {
            ExecutionState::Cancelled(None)
        } else if let Some(parent) =
            parent.filter(|parent| !rows[parent.index()].state.admitted_directory())
        {
            ExecutionState::BlockedByParent(parent)
        } else {
            progress(ExecutionProgress::Preparing(id));
            match execute_entry(
                entry,
                &rows,
                control,
                &mut progress,
                &mut before_directory_create,
                &mut journal,
            ) {
                Ok(state) => state,
                Err(error) if matches!(error.failure, CopyFailure::Cancelled) => {
                    ExecutionState::Cancelled(Some(error))
                }
                Err(error) => {
                    journal_failed = matches!(error.failure, CopyFailure::Journal(_));
                    ExecutionState::Failed(error)
                }
            }
        };

        let created = matches!(state, ExecutionState::CreatedDirectory(_));
        rows[id.index()].state = state;
        if created {
            progress(ExecutionProgress::DirectoryCreated(id));
        }
    }
    if journal_failed {
        for row in &mut rows {
            if let ExecutionState::CreatedDirectory(directory) = &mut row.state {
                directory.times = DirectoryTimes::Incomplete;
            }
        }
    } else {
        finalize_directory_times(plan, control, &mut rows, &mut progress);
    }
    Ok(CopyExecution {
        rows,
        cancelled: control.is_cancelled(),
    })
}

fn execute_entry(
    entry: &ApprovedTransferEntry,
    rows: &[ExecutionRow],
    control: &CopyControl,
    progress: &mut impl FnMut(ExecutionProgress),
    before_directory_create: &mut impl FnMut(&Path),
    journal: &mut Option<&mut dyn TransferJournal>,
) -> Result<ExecutionState, CopyError> {
    let id = entry.id();
    if let ApprovedTarget::Merge { target, directory } = entry.destination() {
        return admit_merge(entry.source(), target, directory, control)
            .map(ExecutionState::MergedDirectory)
            .map_err(CopyError::from);
    }

    let target = match entry.destination() {
        ApprovedTarget::CreateAt(target) | ApprovedTarget::Replace(target) => target.clone(),
        ApprovedTarget::CreateIn { parent, leaf } => {
            let ExecutionState::CreatedDirectory(created) = &rows[parent.index()].state else {
                return Err(CopyFailure::UnsupportedPlan.into());
            };
            inspect_target(&created.snapshot, leaf, control.cancellation())
                .map_err(CopyFailure::from)?
        }
        _ => return Err(CopyFailure::UnsupportedPlan.into()),
    };
    if !matches!(entry.destination(), ApprovedTarget::Replace(_))
        && !matches!(target.occupancy(), TargetOccupancy::Vacant)
    {
        return Err(CopyFailure::DestinationOccupied.into());
    }
    if entry.source().metadata().kind == EntryKind::Directory {
        if matches!(entry.destination(), ApprovedTarget::Replace(_)) {
            return Err(CopyFailure::UnsupportedPlan.into());
        }
        create_directory(entry.source(), &target, control, before_directory_create)
            .map(ExecutionState::CreatedDirectory)
    } else {
        let callback = std::cell::RefCell::new(progress);
        let report = |staged_bytes| {
            (callback.borrow_mut())(ExecutionProgress::Copying {
                entry: id,
                staged_bytes,
            })
        };
        let verify = |verification_requested| {
            (callback.borrow_mut())(ExecutionProgress::Verifying {
                entry: id,
                verification_requested,
            })
        };
        let copied = match entry.destination() {
            ApprovedTarget::CreateAt(_) => {
                copy_to_vacancy_reporting(entry, control, report, verify)?
            }
            ApprovedTarget::CreateIn { .. } => {
                copy_to_created_parent(entry, &target, control, report, verify)?
            }
            ApprovedTarget::Replace(_) => {
                let Some(journal) = journal else {
                    return Err(CopyFailure::UnsupportedPlan.into());
                };
                copy_to_replacement_reporting(entry, control, report, verify, &mut **journal)?
            }
            ApprovedTarget::Merge { .. } => return Err(CopyFailure::UnsupportedPlan.into()),
        };
        (callback.borrow_mut())(ExecutionProgress::FileCompleted {
            entry: id,
            bytes: copied.bytes,
        });
        Ok(ExecutionState::Copied(copied))
    }
}

fn open_source_directory(
    source: &EntrySnapshot,
    control: &CopyControl,
) -> Result<HeldParent, CopyFailure> {
    control.check()?;
    let held = HeldParent::open(&source.path(), control.cancellation())?;
    if held.snapshot.identity() != source.identity()
        || held
            .snapshot
            .chain
            .split_last()
            .map(|(_, ancestors)| ancestors)
            != Some(source.parent().chain.as_ref())
    {
        return Err(AdmissionError::StaleEntry.into());
    }
    check_source_directory(&held, source, control)?;
    Ok(held)
}

fn check_source_directory(
    held: &HeldParent,
    source: &EntrySnapshot,
    control: &CopyControl,
) -> Result<(), CopyFailure> {
    held.check_chain(control.cancellation())?;
    let file = held.directory().file();
    let current = observe_entry_metadata(file)?;
    if current != source.metadata() {
        return Err(AdmissionError::StaleEntry.into());
    }
    require_no_directory_streams(file)?;
    if current.attributes != FILE_ATTRIBUTE_DIRECTORY.0 {
        return Err(CopyFailure::UnsupportedMetadata);
    }
    control.check()
}

fn admit_merge(
    source: &EntrySnapshot,
    target: &TargetSnapshot,
    directory: &ParentSnapshot,
    control: &CopyControl,
) -> Result<ParentSnapshot, CopyFailure> {
    let held_source = open_source_directory(source, control)?;
    let parent = HeldParent::open(target.parent().path(), control.cancellation())?;
    let held = HeldParent::open(directory.path(), control.cancellation())?;
    if parent.snapshot != *target.parent()
        || held.snapshot != *directory
        || &parent.inspect_leaf(target.requested_leaf(), control.cancellation())?
            != target.occupancy()
    {
        return Err(AdmissionError::StaleTarget.into());
    }
    held.check_chain(control.cancellation())?;
    held_source.check_chain(control.cancellation())?;
    Ok(held.snapshot)
}

fn create_directory(
    source: &EntrySnapshot,
    target: &TargetSnapshot,
    control: &CopyControl,
    before_creation: impl FnOnce(&Path),
) -> Result<DirectoryCopy, CopyError> {
    let source_directory = open_source_directory(source, control)?;
    let target_parent = HeldParent::open_with_access(
        target.parent().path(),
        control.cancellation(),
        DirectoryAccess::CreateDirectory,
    )
    .map_err(CopyFailure::from)?;
    if target_parent.snapshot != *target.parent() {
        return Err(CopyFailure::from(AdmissionError::StaleParent).into());
    }
    if !matches!(
        target_parent
            .inspect_leaf(target.requested_leaf(), control.cancellation())
            .map_err(CopyFailure::from)?,
        TargetOccupancy::Vacant
    ) {
        return Err(CopyFailure::DestinationOccupied.into());
    }
    let expected_path = target_parent
        .snapshot
        .path()
        .join(target.requested_leaf().as_os_str());
    before_creation(&expected_path);
    let claim = loop {
        control.check()?;
        let Some(epoch) = control.admission_epoch()? else {
            continue;
        };
        check_source_directory(&source_directory, source, control)?;
        target_parent
            .check_chain(control.cancellation())
            .map_err(CopyFailure::from)?;
        if let Some(claim) = control.try_claim(epoch)? {
            break claim;
        }
    };
    let created_result = target_parent
        .directory()
        .create_directory(target.requested_leaf().as_os_str());
    drop(claim);
    let created = created_result.map_err(|error| directory_create_error(target, error))?;
    let output = CopyArtifact {
        path: expected_path,
        parent: target_parent.snapshot.identity(),
        identity: created.identity(),
        state: CopyArtifactState::Published,
    };

    let result = (|| {
        control.check()?;
        target_parent.check_chain(control.cancellation())?;
        let stamp = directory_stamp(&created, output.parent.volume_serial_number())?;
        let path = checked_final_path(created.file(), &output.path)?;
        require_no_directory_streams(created.file())?;
        let mut chain = target_parent.snapshot.chain.to_vec();
        chain.push(stamp);
        Ok(ParentSnapshot {
            path: path.into(),
            chain: chain.into(),
        })
    })();
    let snapshot = result.map_err(|failure| CopyError {
        failure,
        artifact: Some(if output.identity.file_id() == [0; 16] {
            CopyRecoveryArtifact::UnprovenDirectory {
                path: output.path.clone(),
                parent: output.parent,
                creation_observed: true,
            }
        } else {
            CopyRecoveryArtifact::Owned(output.clone())
        }),
        recovery_error: None,
        replacement: None,
    })?;
    Ok(DirectoryCopy {
        output,
        times: DirectoryTimes::Pending,
        snapshot,
    })
}

fn directory_create_error(
    target: &TargetSnapshot,
    failed: TransferDirectoryCreateFailure,
) -> CopyError {
    let failure = if failed.error.kind() == io::ErrorKind::AlreadyExists
        && failed.outcome == TransferDirectoryCreateOutcome::NotCreated
    {
        CopyFailure::DestinationOccupied
    } else {
        failed.error.into()
    };
    let parent = target.parent();
    CopyError {
        failure,
        artifact: (failed.outcome != TransferDirectoryCreateOutcome::NotCreated).then(|| {
            CopyRecoveryArtifact::UnprovenDirectory {
                path: parent.path().join(target.requested_leaf().as_os_str()),
                parent: parent.identity(),
                creation_observed: failed.outcome == TransferDirectoryCreateOutcome::Observed,
            }
        }),
        recovery_error: None,
        replacement: None,
    }
}

fn finalize_directory_times(
    plan: &ApprovedTransferPlan,
    control: &CopyControl,
    rows: &mut [ExecutionRow],
    progress: &mut impl FnMut(ExecutionProgress),
) {
    let mut incomplete = vec![false; rows.len()];
    for entry in plan.entries().iter().rev() {
        let index = entry.id().index();
        let state = &mut rows[index].state;
        let complete = match state {
            ExecutionState::Copied(_) | ExecutionState::Skipped => true,
            ExecutionState::CreatedDirectory(created) => {
                created.times = if incomplete[index] || control.is_cancelled() {
                    DirectoryTimes::Incomplete
                } else {
                    progress(ExecutionProgress::Finalizing(entry.id()));
                    match set_directory_times(entry.source(), created, control) {
                        Ok(()) => DirectoryTimes::Preserved,
                        Err(CopyFailure::Cancelled) => DirectoryTimes::Incomplete,
                        Err(failure) => DirectoryTimes::Failed(failure),
                    }
                };
                matches!(created.times, DirectoryTimes::Preserved)
            }
            ExecutionState::MergedDirectory(directory) if !incomplete[index] => {
                progress(ExecutionProgress::Finalizing(entry.id()));
                let decision = open_source_directory(entry.source(), control).and_then(|_held| {
                    let destination = HeldParent::open(directory.path(), control.cancellation())?;
                    if destination.snapshot != *directory {
                        return Err(AdmissionError::StaleParent.into());
                    }
                    Ok(())
                });
                if let Err(failure) = decision {
                    *state = if matches!(failure, CopyFailure::Cancelled) {
                        ExecutionState::Cancelled(None)
                    } else {
                        ExecutionState::Failed(failure.into())
                    };
                    false
                } else {
                    true
                }
            }
            _ => false,
        };
        if !complete
            && let Some(parent) = plan
                .source()
                .get(entry.id())
                .and_then(|source| source.parent())
        {
            incomplete[parent.index()] = true;
        }
    }
}

fn set_directory_times(
    source: &EntrySnapshot,
    created: &DirectoryCopy,
    control: &CopyControl,
) -> Result<(), CopyFailure> {
    let held_source = open_source_directory(source, control)?;
    let held = HeldParent::open_with_access(
        created.snapshot.path(),
        control.cancellation(),
        DirectoryAccess::WriteDirectoryMetadata,
    )?;
    let file = held.directory().file();
    let claim = loop {
        control.check()?;
        let Some(epoch) = control.admission_epoch()? else {
            continue;
        };
        check_source_directory(&held_source, source, control)?;
        held.check_chain(control.cancellation())?;
        if held.snapshot != created.snapshot {
            return Err(AdmissionError::StaleParent.into());
        }
        if observe_entry_metadata(file)?.attributes != FILE_ATTRIBUTE_DIRECTORY.0 {
            return Err(CopyFailure::UnsupportedMetadata);
        }
        require_no_directory_streams(file)?;
        if let Some(claim) = control.try_claim(epoch)? {
            break claim;
        }
    };
    let metadata = source.metadata();
    let result = set_replace_basic_info(
        file,
        FILE_BASIC_INFO {
            CreationTime: metadata.created,
            LastWriteTime: metadata.modified,
            ..Default::default()
        },
    );
    drop(claim);
    result?;
    let dates = observe_entry_metadata(file)?;
    if dates.created != metadata.created || dates.modified != metadata.modified {
        return Err(CopyFailure::UnsupportedMetadata);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_operations::ScanCancellation;
    use crate::file_transfer::{
        ReviewChoice, ReviewState, TransferDraft, TransferReview, inspect_source,
        inspect_source_tree, review_transfer,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn review(paths: &[PathBuf], action: TransferAction) -> TransferReview {
        let cancellation = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(paths, &cancellation).unwrap());
        review_transfer(
            tree,
            TransferDraft {
                action,
                verify: true,
            },
            &cancellation,
        )
        .unwrap()
    }

    fn approve(review: &TransferReview) -> ApprovedTransferPlan {
        review
            .approve(review.generation(), &ScanCancellation::new())
            .unwrap()
    }

    #[test]
    fn transfer_execute_native_reports_verification_before_completed_file() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("input.txt");
        let destination = temp.path().join("destination");
        fs::write(&source, b"abc").unwrap();
        fs::create_dir(&destination).unwrap();
        let plan = approve(&review(&[source], TransferAction::CopyTo(destination)));
        let mut events = Vec::new();
        let execution =
            execute_copy_plan(&plan, &CopyControl::default(), |event| events.push(event)).unwrap();
        assert!(matches!(execution.rows[0].state, ExecutionState::Copied(_)));
        let verify = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    ExecutionProgress::Verifying {
                        verification_requested: true,
                        ..
                    }
                )
            })
            .unwrap();
        let done = events
            .iter()
            .position(|event| matches!(event, ExecutionProgress::FileCompleted { bytes: 3, .. }))
            .unwrap();
        assert!(verify < done);
    }

    #[test]
    fn transfer_execute_native_cancel_at_directory_finalization_retains_copied_children() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("input");
        let destination = temp.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("child.txt"), b"copied").unwrap();
        let plan = approve(&review(
            &[source],
            TransferAction::CopyTo(destination.clone()),
        ));
        let control = CopyControl::default();
        let report = execute_copy_plan(&plan, &control, |event| {
            if matches!(event, ExecutionProgress::Finalizing(entry) if entry.index() == 0) {
                control.cancel();
            }
        })
        .unwrap();
        assert!(report.cancelled);
        assert!(matches!(
            &report.rows[0].state,
            ExecutionState::CreatedDirectory(DirectoryCopy {
                times: DirectoryTimes::Incomplete,
                ..
            })
        ));
        assert!(matches!(&report.rows[1].state, ExecutionState::Copied(_)));
        assert_eq!(
            fs::read(destination.join("input/child.txt")).unwrap(),
            b"copied"
        );
    }

    #[test]
    fn transfer_execute_native_nested_create_binds_owned_parents_and_preserves_times() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("Folder");
        let destination = fixture.path().join("output");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::create_dir_all(root.join("empty")).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(root.join("nested").join("inside.txt"), b"nested").unwrap();
        fs::write(root.join("top.txt"), b"top").unwrap();

        let plan = approve(&review(
            &[root],
            TransferAction::CopyTo(destination.clone()),
        ));
        let outcome = execute_copy_plan(&plan, &CopyControl::default(), |_| {}).unwrap();
        assert!(!outcome.cancelled);
        assert_eq!(outcome.rows.len(), 5);
        for row in outcome.rows {
            match row.state {
                ExecutionState::CreatedDirectory(dir) => {
                    assert!(matches!(dir.times, DirectoryTimes::Preserved), "{dir:?}");
                    let output =
                        inspect_source(&dir.output.path, &ScanCancellation::new()).unwrap();
                    let source = plan
                        .entries()
                        .iter()
                        .find(|entry| entry.id() == row.entry)
                        .unwrap()
                        .source();
                    assert_eq!(output.identity(), dir.output.identity);
                    assert_eq!(output.metadata().created, source.metadata().created);
                    assert_eq!(output.metadata().modified, source.metadata().modified);
                }
                ExecutionState::Copied(_) => {}
                state => panic!("nested copy failed: {state:?}"),
            }
        }
        let result = destination.join("Folder");
        assert_eq!(
            fs::read(result.join("nested").join("inside.txt")).unwrap(),
            b"nested"
        );
        assert_eq!(fs::read(result.join("top.txt")).unwrap(), b"top");
        assert_eq!(fs::read_dir(result.join("empty")).unwrap().count(), 0);
    }

    #[test]
    fn transfer_execute_native_late_directory_is_preserved_and_its_descendants_blocked() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("Folder");
        let independent = fixture.path().join("other.txt");
        let destination = fixture.path().join("output");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(root.join("child.txt"), b"reviewed child").unwrap();
        fs::write(&independent, b"other").unwrap();
        let plan = approve(&review(
            &[root, independent],
            TransferAction::CopyTo(destination.clone()),
        ));
        let outcome = execute_copy_with_hooks(
            &plan,
            &CopyControl::default(),
            |_| {},
            |path| {
                fs::create_dir(path).unwrap();
                fs::write(path.join("foreign.txt"), b"foreign").unwrap();
            },
            None,
        )
        .unwrap();
        assert!(matches!(&outcome.rows[0].state,
            ExecutionState::Failed(error) if matches!(error.failure, CopyFailure::DestinationOccupied) && error.artifact.is_none()));
        assert!(
            matches!(&outcome.rows[1].state, ExecutionState::BlockedByParent(id) if *id == outcome.rows[0].entry)
        );
        assert!(matches!(outcome.rows[2].state, ExecutionState::Copied(_)));
        assert_eq!(
            fs::read(destination.join("Folder").join("foreign.txt")).unwrap(),
            b"foreign"
        );
        assert!(!destination.join("Folder").join("child.txt").exists());
        assert_eq!(fs::read(destination.join("other.txt")).unwrap(), b"other");
    }

    #[test]
    fn transfer_execute_native_replaced_owned_directory_cannot_admit_children() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("Folder");
        let destination = fixture.path().join("output");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(root.join("child.txt"), b"source child").unwrap();
        let plan = approve(&review(
            &[root],
            TransferAction::CopyTo(destination.clone()),
        ));
        let output = destination.join("Folder");
        let earlier = destination.join("earlier");
        let outcome = execute_copy_plan(&plan, &CopyControl::default(), |event| {
            if matches!(event, ExecutionProgress::DirectoryCreated(_)) {
                fs::rename(&output, &earlier).unwrap();
                fs::create_dir(&output).unwrap();
                fs::write(output.join("foreign.txt"), b"foreign").unwrap();
            }
        })
        .unwrap();
        assert!(matches!(&outcome.rows[1].state,
            ExecutionState::Failed(error) if matches!(error.failure, CopyFailure::Admission(AdmissionError::StaleParent))));
        assert!(matches!(&outcome.rows[0].state,
            ExecutionState::CreatedDirectory(dir) if matches!(dir.times, DirectoryTimes::Incomplete)));
        assert_eq!(fs::read_dir(earlier).unwrap().count(), 0);
        assert_eq!(fs::read(output.join("foreign.txt")).unwrap(), b"foreign");
        assert!(!output.join("child.txt").exists());
    }

    #[test]
    fn transfer_execute_native_approved_merge_preserves_unrelated_contents() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("Folder");
        let destination = fixture.path().join("output");
        let existing = destination.join("FOLDER");
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(&existing).unwrap();
        fs::write(root.join("new.txt"), b"new").unwrap();
        fs::write(existing.join("foreign.txt"), b"keep").unwrap();
        let review = review(&[root], TransferAction::CopyTo(destination));
        assert!(matches!(review.rows()[0].state(), ReviewState::Conflict(_)));
        let review = review
            .choose(
                review.generation(),
                review.rows()[0].source(),
                ReviewChoice::Merge,
                &ScanCancellation::new(),
            )
            .unwrap();
        let plan = approve(&review);
        let outcome = execute_copy_plan(&plan, &CopyControl::default(), |_| {}).unwrap();
        assert!(matches!(
            outcome.rows[0].state,
            ExecutionState::MergedDirectory(_)
        ));
        assert!(matches!(outcome.rows[1].state, ExecutionState::Copied(_)));
        assert_eq!(fs::read(existing.join("foreign.txt")).unwrap(), b"keep");
        assert_eq!(fs::read(existing.join("new.txt")).unwrap(), b"new");
    }

    #[test]
    fn transfer_execute_native_merge_rechecks_a_source_changed_after_admission() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("Folder");
        let destination = fixture.path().join("output");
        let existing = destination.join("Folder");
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(&existing).unwrap();
        fs::write(root.join("reviewed.txt"), b"reviewed").unwrap();
        let review = review(
            std::slice::from_ref(&root),
            TransferAction::CopyTo(destination),
        );
        let review = review
            .choose(
                review.generation(),
                review.rows()[0].source(),
                ReviewChoice::Merge,
                &ScanCancellation::new(),
            )
            .unwrap();
        let plan = approve(&review);
        let child = plan.entries()[1].id();
        let outcome = execute_copy_plan(&plan, &CopyControl::default(), |event| {
            if matches!(event, ExecutionProgress::Preparing(id) if id == child) {
                fs::write(root.join("new.txt"), b"arrived late").unwrap();
            }
        })
        .unwrap();
        assert!(
            matches!(&outcome.rows[0].state, ExecutionState::Failed(error)
                if matches!(error.failure, CopyFailure::Admission(AdmissionError::StaleEntry))),
            "{:?}",
            outcome.rows[0],
        );
        assert!(matches!(outcome.rows[1].state, ExecutionState::Copied(_)));
        assert_eq!(
            fs::read(existing.join("reviewed.txt")).unwrap(),
            b"reviewed"
        );
        assert!(!existing.join("new.txt").exists());
        assert_eq!(fs::read(root.join("new.txt")).unwrap(), b"arrived late");
    }

    #[test]
    fn transfer_execute_native_directory_stream_is_refused_before_creation() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("Folder");
        let destination = fixture.path().join("output");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&destination).unwrap();
        let mut ads = root.clone().into_os_string();
        ads.push(":FixtureAlternate:$DATA");
        fs::write(Path::new(&ads), b"directory stream").unwrap();
        let plan = approve(&review(
            &[root],
            TransferAction::CopyTo(destination.clone()),
        ));
        let outcome = execute_copy_plan(&plan, &CopyControl::default(), |_| {}).unwrap();
        assert!(
            matches!(&outcome.rows[0].state,
            ExecutionState::Failed(error) if matches!(error.failure, CopyFailure::UnsupportedStreams)),
            "{:?}",
            outcome.rows[0]
        );
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
        assert_eq!(fs::read(Path::new(&ads)).unwrap(), b"directory stream");
    }

    struct ReplacementFixture {
        _temp: tempfile::TempDir,
        destination: std::path::PathBuf,
        incumbent: std::path::PathBuf,
        plan: ApprovedTransferPlan,
    }

    fn replacement_fixture(with_unrelated: bool) -> ReplacementFixture {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("destination");
        fs::create_dir(&destination).unwrap();
        let source = temp.path().join("reviewed.txt");
        let incumbent = destination.join("Reviewed.TXT");
        fs::write(&source, b"replacement source").unwrap();
        fs::write(&incumbent, b"original incumbent").unwrap();
        // Distinct creation times reproduce NTFS's former-name timestamp
        // tunneling independently of the scheduler and the current clock.
        for (path, seconds) in [(&source, 1_010_000_000), (&incumbent, 1_410_000_000)] {
            let file = fs::OpenOptions::new().write(true).open(path).unwrap();
            let timestamp = (11_644_473_600_i64 + seconds) * 10_000_000;
            set_replace_basic_info(
                &file,
                FILE_BASIC_INFO {
                    CreationTime: timestamp,
                    LastWriteTime: timestamp,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let mut sources = vec![source];
        if with_unrelated {
            let other = temp.path().join("other.txt");
            fs::write(&other, b"independent").unwrap();
            sources.push(other);
        }
        let review = review(&sources, TransferAction::CopyTo(destination.clone()));
        let review = review
            .choose(
                review.generation(),
                review.rows()[0].source(),
                ReviewChoice::Replace,
                &ScanCancellation::new(),
            )
            .unwrap();
        ReplacementFixture {
            _temp: temp,
            destination,
            incumbent,
            plan: approve(&review),
        }
    }

    use super::super::journal::TransferJournalEvent as JournalEvent;
    use super::super::{CopyVerification, ReplacementState};

    struct NativeJournal<F> {
        accepted: Vec<JournalEvent>,
        on_record: F,
    }

    impl<F: FnMut(&JournalEvent) -> io::Result<()>> TransferJournal for NativeJournal<F> {
        fn record(&mut self, event: JournalEvent) -> io::Result<()> {
            (self.on_record)(&event)?;
            self.accepted.push(event);
            Ok(())
        }
    }

    fn run_replace(
        fixture: &ReplacementFixture,
        control: &CopyControl,
        journal: &mut dyn TransferJournal,
    ) -> CopyExecution {
        execute_copy_with_hooks(&fixture.plan, control, |_| {}, |_| {}, Some(journal)).unwrap()
    }

    #[test]
    fn transfer_replace_native_verified_publish_retains_exact_incumbent() {
        let fixture = replacement_fixture(false);
        let mut journal = NativeJournal {
            accepted: Vec::new(),
            on_record: |_: &JournalEvent| Ok(()),
        };
        let report = run_replace(&fixture, &CopyControl::default(), &mut journal);
        let ExecutionState::Copied(copied) = &report.rows[0].state else {
            panic!("expected guarded replacement, got {:?}", report.rows[0]);
        };
        let backup = copied.replacement.as_ref().unwrap();
        assert_eq!(backup.state, ReplacementState::Retained);
        assert_eq!(backup.original, fixture.incumbent);
        assert_eq!(copied.output.path.file_name().unwrap(), "reviewed.txt");
        assert_eq!(
            fs::read(&copied.output.path).unwrap(),
            b"replacement source"
        );
        assert_eq!(fs::read(&backup.backup).unwrap(), b"original incumbent");
        let cancel = ScanCancellation::new();
        let backup_now = super::super::inspect_source(&backup.backup, &cancel).unwrap();
        let output_now = super::super::inspect_source(&copied.output.path, &cancel).unwrap();
        assert_eq!(backup_now.identity(), backup.identity);
        assert_eq!(output_now.identity(), copied.output.identity);
        let source_metadata = fixture.plan.entries()[0].source().metadata();
        assert_eq!(output_now.metadata().created, source_metadata.created);
        assert_eq!(output_now.metadata().modified, source_metadata.modified);
        assert_ne!(backup_now.metadata().created, source_metadata.created);
        assert!(matches!(
            copied.verification,
            CopyVerification::Verified { .. }
        ));
        assert!(matches!(journal.accepted.as_slice(), [
            JournalEvent::StageIntent { path: candidate, .. },
            JournalEvent::StageCreated { path: stage, identity: stage_id, .. },
            JournalEvent::DisplacementIntent { identity: incumbent, .. },
            JournalEvent::DisplacementObserved { identity: displaced, .. },
            JournalEvent::PublishIntent { identity: expected, .. },
            JournalEvent::PublicationObserved { identity: published, .. },
            JournalEvent::Verified { identity: verified, verification: CopyVerification::Verified { .. }, bytes: 18, .. },
        ] if candidate == stage && incumbent == displaced && incumbent == &backup.identity
            && stage_id == expected && expected == published && published == verified));
    }

    #[test]
    fn transfer_replace_native_cancel_restores_original_spelling() {
        let fixture = replacement_fixture(false);
        let control = CopyControl::default();
        let mut journal = NativeJournal {
            accepted: Vec::new(),
            on_record: |event: &JournalEvent| {
                if matches!(event, JournalEvent::DisplacementObserved { .. }) {
                    control.cancel();
                }
                Ok(())
            },
        };
        let report = run_replace(&fixture, &control, &mut journal);
        let ExecutionState::Cancelled(Some(error)) = &report.rows[0].state else {
            panic!(
                "expected cancellation after relocation, got {:?}",
                report.rows[0]
            );
        };
        assert!(matches!(error.failure, CopyFailure::Cancelled));
        assert!(error.artifact.is_none());
        let backup = error.replacement.as_ref().unwrap();
        assert_eq!(backup.state, ReplacementState::Restored);
        assert_eq!(fs::read(&fixture.incumbent).unwrap(), b"original incumbent");
        let entries: Vec<_> = fs::read_dir(&fixture.destination)
            .unwrap()
            .map(|result| result.unwrap().file_name())
            .collect();
        assert_eq!(entries, ["Reviewed.TXT"]);
        assert!(
            matches!(journal.accepted.last(), Some(JournalEvent::RestoreObserved { original, .. }) if original == &fixture.incumbent)
        );
        assert!(
            journal
                .accepted
                .iter()
                .all(|event| !matches!(event, JournalEvent::PublicationObserved { .. }))
        );
    }

    #[test]
    fn transfer_replace_native_foreign_winner_survives_and_backup_is_retained() {
        let fixture = replacement_fixture(false);
        let mut journal = NativeJournal {
            accepted: Vec::new(),
            on_record: |event: &JournalEvent| {
                if matches!(event, JournalEvent::DisplacementObserved { .. }) {
                    fs::write(&fixture.incumbent, b"late foreign winner").unwrap();
                }
                Ok(())
            },
        };
        let report = run_replace(&fixture, &CopyControl::default(), &mut journal);
        let ExecutionState::Failed(error) = &report.rows[0].state else {
            panic!("expected late conflict, got {:?}", report.rows[0]);
        };
        assert!(matches!(error.failure, CopyFailure::DestinationOccupied));
        assert!(error.artifact.is_none());
        let backup = error.replacement.as_ref().unwrap();
        assert_eq!(backup.state, ReplacementState::Retained);
        assert_eq!(
            fs::read(&fixture.incumbent).unwrap(),
            b"late foreign winner"
        );
        assert_eq!(fs::read(&backup.backup).unwrap(), b"original incumbent");
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 2);
    }

    #[test]
    fn transfer_replace_native_journal_failure_halts_later_namespace_effects() {
        let fixture = replacement_fixture(true);
        let mut journal = NativeJournal {
            accepted: Vec::new(),
            on_record: |event: &JournalEvent| {
                if matches!(event, JournalEvent::DisplacementObserved { .. }) {
                    return Err(io::Error::other("fixture receipt unavailable after rename"));
                }
                Ok(())
            },
        };
        let report = run_replace(&fixture, &CopyControl::default(), &mut journal);
        let ExecutionState::Failed(error) = &report.rows[0].state else {
            panic!(
                "expected receipt failure after relocation, got {:?}",
                report.rows[0]
            );
        };
        assert!(matches!(error.failure, CopyFailure::Journal(_)));
        assert!(error.artifact.is_none());
        let backup = error.replacement.as_ref().unwrap();
        assert_eq!(backup.state, ReplacementState::Retained);
        assert!(!fixture.incumbent.exists());
        assert!(matches!(
            report.rows[1].state,
            ExecutionState::BlockedByReceipt
        ));
        assert!(!fixture.destination.join("other.txt").exists());
        assert_eq!(fs::read(&backup.backup).unwrap(), b"original incumbent");
        assert!(matches!(
            journal.accepted.last(),
            Some(JournalEvent::DisplacementIntent { .. })
        ));
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 1);
    }

    #[test]
    fn transfer_replace_native_stage_intent_refusal_has_no_effect() {
        let fixture = replacement_fixture(false);
        let mut journal = NativeJournal {
            accepted: Vec::new(),
            on_record: |_: &JournalEvent| Err(io::Error::other("fixture receipt unavailable")),
        };
        let report = run_replace(&fixture, &CopyControl::default(), &mut journal);
        assert!(matches!(&report.rows[0].state,
            ExecutionState::Failed(error) if matches!(error.failure, CopyFailure::Journal(_))
                && error.replacement.is_none() && error.artifact.is_none()));
        assert!(journal.accepted.is_empty());
        assert_eq!(fs::read(&fixture.incumbent).unwrap(), b"original incumbent");
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 1);
    }

    #[test]
    fn transfer_replace_native_same_length_replacement_is_stale() {
        let fixture = replacement_fixture(false);
        let moved = fixture.destination.join("moved-original.txt");
        fs::rename(&fixture.incumbent, &moved).unwrap();
        fs::write(&fixture.incumbent, b"different new file").unwrap();
        let mut journal = NativeJournal {
            accepted: Vec::new(),
            on_record: |_: &JournalEvent| Ok(()),
        };
        let report = run_replace(&fixture, &CopyControl::default(), &mut journal);
        assert!(
            matches!(&report.rows[0].state,
            ExecutionState::Failed(error) if matches!(error.failure, CopyFailure::Admission(AdmissionError::StaleTarget))
                && error.artifact.is_none() && error.replacement.is_none()),
            "{:?}",
            report.rows[0]
        );
        assert!(journal.accepted.is_empty());
        assert_eq!(fs::read(fixture.incumbent).unwrap(), b"different new file");
        assert_eq!(fs::read(moved).unwrap(), b"original incumbent");
        assert_eq!(fs::read_dir(fixture.destination).unwrap().count(), 2);
    }

    #[test]
    fn transfer_replace_native_late_receipt_failure_retains_both_versions() {
        let fixture = replacement_fixture(false);
        let mut journal = NativeJournal {
            accepted: Vec::new(),
            on_record: |event: &JournalEvent| {
                if matches!(event, JournalEvent::Verified { .. }) {
                    return Err(io::Error::other("fixture verification receipt unavailable"));
                }
                Ok(())
            },
        };
        let report = run_replace(&fixture, &CopyControl::default(), &mut journal);
        let ExecutionState::Failed(error) = &report.rows[0].state else {
            panic!(
                "expected a receipt failure after publication, got {:?}",
                report.rows[0]
            );
        };
        assert!(
            matches!(error.failure, CopyFailure::Journal(_)),
            "actual outcome {error:?}, accepted receipts {:?}",
            journal.accepted
        );
        let Some(CopyRecoveryArtifact::Owned(output)) = error.artifact.as_ref() else {
            panic!("published artifact must be retained");
        };
        let backup = error.replacement.as_ref().unwrap();
        assert_eq!(backup.state, ReplacementState::Retained);
        assert_eq!(output.state, CopyArtifactState::Published);
        assert_eq!(fs::read(&output.path).unwrap(), b"replacement source");
        assert_eq!(fs::read(&backup.backup).unwrap(), b"original incumbent");
        assert!(matches!(
            journal.accepted.last(),
            Some(JournalEvent::PublicationObserved { .. })
        ));
    }

    #[test]
    fn transfer_replace_native_cross_parent_collision_leaves_both_entries_intact() {
        let fixture = tempfile::tempdir().unwrap();
        let from = fixture.path().join("from");
        let into = fixture.path().join("into");
        fs::create_dir(&from).unwrap();
        fs::create_dir(&into).unwrap();
        let source = from.join("shared.txt");
        let competitor = into.join("shared.txt");
        fs::write(&source, b"original").unwrap();
        fs::write(&competitor, b"foreign").unwrap();
        let cancel = ScanCancellation::new();
        let snapshot = super::super::inspect_source(&source, &cancel).unwrap();
        let original = HeldParent::open(&from, &cancel).unwrap();
        let destination =
            HeldParent::open_with_access(&into, &cancel, DirectoryAccess::CreateFile).unwrap();
        let held = original
            .directory()
            .open_relocation(snapshot.leaf().as_os_str(), snapshot.identity())
            .unwrap();
        assert!(fs::OpenOptions::new().write(true).open(&source).is_err());
        let error = held
            .rename_no_replace(destination.directory(), snapshot.leaf().as_os_str())
            .expect_err("a foreign destination must survive");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(!error.unconfirmed_name_effect());
        assert!(checked_final_path(held.file(), &source).is_ok());
        assert_eq!(held.identity(), snapshot.identity());
        drop(held);
        assert_eq!(fs::read(source).unwrap(), b"original");
        assert_eq!(fs::read(competitor).unwrap(), b"foreign");
    }

    #[test]
    fn transfer_replace_native_exact_stage_does_not_probe_another_name() {
        let fixture = tempfile::tempdir().unwrap();
        let candidate = fixture.path().join("exact.stage");
        fs::write(&candidate, b"foreign candidate").unwrap();
        let directory = HeldParent::open_with_access(
            fixture.path(),
            &ScanCancellation::new(),
            DirectoryAccess::CreateFile,
        )
        .unwrap();
        let error = directory
            .directory()
            .create_stage_exact(
                std::ffi::OsStr::new("output.txt"),
                candidate.file_name().unwrap(),
            )
            .err()
            .expect("an occupied exact stage must fail");
        assert_eq!(error.error.kind(), io::ErrorKind::AlreadyExists);
        assert!(error.unresolved.is_none());
        assert_eq!(fs::read(&candidate).unwrap(), b"foreign candidate");
        assert_eq!(fs::read_dir(fixture.path()).unwrap().count(), 1);
    }

    #[test]
    fn transfer_execute_native_unsupported_plans_are_rejected_before_any_write() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source.txt");
        let other = fixture.path().join("first.txt");
        let destination = fixture.path().join("output");
        fs::create_dir(&destination).unwrap();
        fs::write(&source, b"source").unwrap();
        fs::write(&other, b"first").unwrap();
        let mode = review(
            std::slice::from_ref(&source),
            TransferAction::MoveTo(destination.clone()),
        );
        assert!(matches!(
            execute_copy_plan(&approve(&mode), &CopyControl::default(), |_| {}),
            Err(CopyFailure::UnsupportedPlan)
        ));
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);

        fs::write(destination.join("source.txt"), b"foreign").unwrap();
        let review = review(
            &[other, source],
            TransferAction::CopyTo(destination.clone()),
        );
        let review = review
            .choose(
                review.generation(),
                review.rows()[1].source(),
                ReviewChoice::Replace,
                &ScanCancellation::new(),
            )
            .unwrap();
        assert!(matches!(
            execute_copy_plan(&approve(&review), &CopyControl::default(), |_| {}),
            Err(CopyFailure::UnsupportedPlan)
        ));
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 1);
        assert_eq!(
            fs::read(destination.join("source.txt")).unwrap(),
            b"foreign"
        );
    }
}
