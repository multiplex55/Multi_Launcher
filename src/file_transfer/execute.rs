//! Sequential execution of approved copy and duplicate entries.

use std::io;
use std::path::Path;

use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_BASIC_INFO};

use crate::archive::transfer_native::{
    TransferDirectoryCreateFailure, TransferDirectoryCreateOutcome,
};
use crate::file_operations::set_replace_basic_info;

use super::copy::{copy_to_created_parent, require_no_directory_streams};
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
    Failed(CopyError),
    Cancelled,
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
    execute_copy_with_hooks(plan, control, progress, |_| {})
}

fn execute_copy_with_hooks(
    plan: &ApprovedTransferPlan,
    control: &CopyControl,
    mut progress: impl FnMut(ExecutionProgress),
    mut before_directory_create: impl FnMut(&Path),
) -> Result<CopyExecution, CopyFailure> {
    if matches!(plan.draft().action, TransferAction::MoveTo(_))
        || plan
            .entries()
            .iter()
            .any(|entry| matches!(entry.destination(), ApprovedTarget::Replace(_)))
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
    for entry in plan.entries() {
        let id = entry.id();
        let parent = plan.source().get(id).and_then(|source| source.parent());
        let state = if control.is_cancelled() {
            ExecutionState::Cancelled
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
            ) {
                Ok(state) => state,
                Err(error)
                    if error.artifact.is_none()
                        && matches!(error.failure, CopyFailure::Cancelled) =>
                {
                    ExecutionState::Cancelled
                }
                Err(error) => ExecutionState::Failed(error),
            }
        };

        let created = matches!(state, ExecutionState::CreatedDirectory(_));
        rows[id.index()].state = state;
        if created {
            progress(ExecutionProgress::DirectoryCreated(id));
        }
    }
    finalize_directory_times(plan, control, &mut rows);
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
) -> Result<ExecutionState, CopyError> {
    let id = entry.id();
    if let ApprovedTarget::Merge { target, directory } = entry.destination() {
        return admit_merge(entry.source(), target, directory, control)
            .map(ExecutionState::MergedDirectory)
            .map_err(CopyError::from);
    }

    let target = match entry.destination() {
        ApprovedTarget::CreateAt(target) => target.clone(),
        ApprovedTarget::CreateIn { parent, leaf } => {
            let ExecutionState::CreatedDirectory(created) = &rows[parent.index()].state else {
                return Err(CopyFailure::UnsupportedPlan.into());
            };
            inspect_target(&created.snapshot, leaf, control.cancellation())
                .map_err(CopyFailure::from)?
        }
        _ => return Err(CopyFailure::UnsupportedPlan.into()),
    };
    if !matches!(target.occupancy(), TargetOccupancy::Vacant) {
        return Err(CopyFailure::DestinationOccupied.into());
    }
    if entry.source().metadata().kind == EntryKind::Directory {
        create_directory(entry.source(), &target, control, before_directory_create)
            .map(ExecutionState::CreatedDirectory)
    } else {
        let report = |staged_bytes| {
            progress(ExecutionProgress::Copying {
                entry: id,
                staged_bytes,
            })
        };
        let copied = match entry.destination() {
            ApprovedTarget::CreateAt(_) => copy_to_vacancy(entry, control, report)?,
            ApprovedTarget::CreateIn { .. } => {
                copy_to_created_parent(entry, &target, control, report)?
            }
            _ => return Err(CopyFailure::UnsupportedPlan.into()),
        };
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
    check_source_directory(&source_directory, source, control)?;
    target_parent
        .check_chain(control.cancellation())
        .map_err(CopyFailure::from)?;
    let created = target_parent
        .directory()
        .create_directory(target.requested_leaf().as_os_str())
        .map_err(|error| directory_create_error(target, error))?;
    let output = CopyArtifact {
        path: expected_path,
        parent: target_parent.snapshot.identity(),
        identity: created.identity(),
        state: CopyArtifactState::Published,
    };

    let result = (|| {
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
    }
}

fn finalize_directory_times(
    plan: &ApprovedTransferPlan,
    control: &CopyControl,
    rows: &mut [ExecutionRow],
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
                    match set_directory_times(entry.source(), created, control) {
                        Ok(()) => DirectoryTimes::Preserved,
                        Err(failure) => DirectoryTimes::Failed(failure),
                    }
                };
                matches!(created.times, DirectoryTimes::Preserved)
            }
            ExecutionState::MergedDirectory(directory) if !incomplete[index] => {
                let decision = open_source_directory(entry.source(), control).and_then(|_held| {
                    let destination = HeldParent::open(directory.path(), control.cancellation())?;
                    if destination.snapshot != *directory {
                        return Err(AdmissionError::StaleParent.into());
                    }
                    Ok(())
                });
                if let Err(failure) = decision {
                    *state = if matches!(failure, CopyFailure::Cancelled) {
                        ExecutionState::Cancelled
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
    let _source = open_source_directory(source, control)?;
    let held = HeldParent::open_with_access(
        created.snapshot.path(),
        control.cancellation(),
        DirectoryAccess::WriteDirectoryMetadata,
    )?;
    if held.snapshot != created.snapshot {
        return Err(AdmissionError::StaleParent.into());
    }
    let file = held.directory().file();
    if observe_entry_metadata(file)?.attributes != FILE_ATTRIBUTE_DIRECTORY.0 {
        return Err(CopyFailure::UnsupportedMetadata);
    }
    require_no_directory_streams(file)?;
    control.check()?;
    let metadata = source.metadata();
    set_replace_basic_info(
        file,
        FILE_BASIC_INFO {
            CreationTime: metadata.created,
            LastWriteTime: metadata.modified,
            ..Default::default()
        },
    )?;
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
