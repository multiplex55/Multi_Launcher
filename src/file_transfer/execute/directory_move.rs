//! Reverse, journaled finalization of directory moves.
use super::*;
use crate::file_operations::{
    NativeFileIdentity, ScanCancellation, TransferEnumerationError, enumerate_transfer_directory,
};
use crate::file_transfer::{SourceObservation, journal::TransferJournalEvent as Event};

#[derive(Debug)]
pub enum DirectoryMoveDestination {
    Created(DirectoryCopy),
    Merged(ParentSnapshot),
}
#[derive(Debug)]
pub struct DirectoryMove {
    pub destination: DirectoryMoveDestination,
    pub source: DirectorySourceOutcome,
}
impl DirectoryMove {
    pub fn completed(&self) -> bool {
        matches!(self.source, DirectorySourceOutcome::Removed { .. })
    }

    pub fn has_retained_effect(&self) -> bool {
        matches!(self.destination, DirectoryMoveDestination::Created(_))
            || matches!(
                self.source,
                DirectorySourceOutcome::Removed { .. }
                    | DirectorySourceOutcome::RemovalUncertain(_)
                    | DirectorySourceOutcome::ReceiptFailed {
                        removal: Some(
                            DirectoryRemovalObservation::Removed
                                | DirectoryRemovalObservation::Replaced(_)
                                | DirectoryRemovalObservation::Uncertain
                        ),
                        ..
                    }
            )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryRemovalObservation {
    Removed,
    Replaced(NativeFileIdentity),
    Retained,
    Uncertain,
}
#[derive(Debug)]
pub enum DirectorySourceOutcome {
    Removed {
        replacement: Option<NativeFileIdentity>,
    },
    RetainedChildren {
        skipped: Vec<SourceNodeId>,
        incomplete: Vec<SourceNodeId>,
    },
    NonEmpty,
    Cancelled,
    ReceiptBlocked,
    ReceiptFailed {
        error: CopyFailure,
        removal: Option<DirectoryRemovalObservation>,
    },
    Failed(CopyFailure),
    RemovalUncertain(CopyFailure),
}

/// Rows must cover the full source tree, indexed by SourceNodeId. A prior
/// receipt failure fences all remaining metadata and source mutations.
pub(super) fn finalize_directory_moves(
    plan: &ApprovedTransferPlan,
    control: &CopyControl,
    rows: &mut [ExecutionRow],
    progress: &mut impl FnMut(ExecutionProgress),
    journal: &mut dyn TransferJournal,
    mut receipt_failed: bool,
) {
    let entries = plan.source().entries();
    if rows.len() != entries.len() {
        return;
    }
    let mut children = vec![Vec::new(); rows.len()];
    for entry in entries {
        if let Some(parent) = entry.parent() {
            children[parent.index()].push(entry.id());
        }
    }
    let mut reviewed_skip = vec![false; rows.len()];
    for id in plan.skipped() {
        reviewed_skip[id.index()] = true;
    }
    let mut skipped_causes = vec![Vec::new(); rows.len()];
    let mut incomplete_causes = vec![Vec::new(); rows.len()];
    for node in entries.iter().rev() {
        let id = node.id();
        let index = id.index();
        if reviewed_skip[index] {
            skipped_causes[index].push(id);
            continue;
        }
        let source = match node.observation() {
            SourceObservation::Ready(source) if source.metadata().kind == EntryKind::Directory => {
                source
            }
            _ => {
                if !matches!(
                    rows[index].state,
                    ExecutionState::Moved(_) | ExecutionState::MovedByCopy(_)
                ) {
                    incomplete_causes[index].push(id);
                }
                continue;
            }
        };
        let old = std::mem::replace(&mut rows[index].state, ExecutionState::Skipped);
        let destination = match old {
            ExecutionState::CreatedDirectory(created) => DirectoryMoveDestination::Created(created),
            ExecutionState::MergedDirectory(snapshot) => DirectoryMoveDestination::Merged(snapshot),
            other => {
                rows[index].state = other;
                incomplete_causes[index].push(id);
                continue;
            }
        };
        let mut moved = DirectoryMove {
            destination,
            source: DirectorySourceOutcome::ReceiptBlocked,
        };
        let mut skipped = Vec::new();
        let mut incomplete = Vec::new();
        for child in &children[index] {
            skipped.extend_from_slice(&skipped_causes[child.index()]);
            incomplete.extend_from_slice(&incomplete_causes[child.index()]);
        }
        if receipt_failed {
            moved.source = DirectorySourceOutcome::ReceiptBlocked;
        } else if control.is_cancelled() {
            moved.source = DirectorySourceOutcome::Cancelled;
        } else {
            progress(ExecutionProgress::Finalizing(id));
            let start = journal
                .begin_operation()
                .and_then(|()| journal.select_entry(id))
                .map_err(CopyFailure::Journal);
            let result = start.and_then(|()| {
                finalize_one(
                    id,
                    source,
                    &mut moved.destination,
                    !incomplete.is_empty(),
                    !skipped.is_empty(),
                    control,
                    journal,
                    || progress(ExecutionProgress::RemovingSource(id)),
                )
            });
            journal.end_operation();
            moved.source = match result {
                Ok(outcome @ DirectorySourceOutcome::ReceiptFailed { .. }) => {
                    receipt_failed = true;
                    outcome
                }
                Ok(DirectorySourceOutcome::RetainedChildren { .. }) => {
                    DirectorySourceOutcome::RetainedChildren {
                        skipped: skipped.clone(),
                        incomplete: incomplete.clone(),
                    }
                }
                Ok(outcome) => outcome,
                Err(CopyFailure::Cancelled) => DirectorySourceOutcome::Cancelled,
                Err(failure @ CopyFailure::Journal(_)) => {
                    receipt_failed = true;
                    DirectorySourceOutcome::Failed(failure)
                }
                Err(failure) => DirectorySourceOutcome::Failed(failure),
            };
        }
        if matches!(
            moved.source,
            DirectorySourceOutcome::RetainedChildren { .. }
        ) {
            skipped_causes[index] = skipped;
            incomplete_causes[index] = incomplete;
        } else if !moved.completed() {
            incomplete_causes[index].push(id);
        }
        if let DirectoryMoveDestination::Created(created) = &mut moved.destination
            && matches!(created.times, DirectoryTimes::Pending)
        {
            created.times = DirectoryTimes::Incomplete;
        }
        rows[index].state = ExecutionState::MovedDirectory(moved);
    }
}

fn finalize_one(
    id: SourceNodeId,
    source: &EntrySnapshot,
    destination: &mut DirectoryMoveDestination,
    has_incomplete: bool,
    has_skipped: bool,
    control: &CopyControl,
    journal: &mut dyn TransferJournal,
    removing_source: impl FnOnce(),
) -> Result<DirectorySourceOutcome, CopyFailure> {
    control.check()?;
    let source_parent = HeldParent::open(source.parent().path(), control.cancellation())?;
    if source_parent.snapshot != *source.parent() {
        return Err(AdmissionError::StaleParent.into());
    }
    let removable = source_parent
        .directory()
        .open_removable_directory_child(source.leaf().as_os_str(), source.identity())?;
    let verify_source = || -> Result<(), CopyFailure> {
        source_parent.check_chain(control.cancellation())?;
        let current = source_parent.capture_entry(removable.file(), source.leaf())?;
        let actual = current.metadata();
        let original = source.metadata();
        if current.identity() != source.identity()
            || current.leaf() != source.leaf()
            || actual.kind != EntryKind::Directory
            || actual.created != original.created
            || actual.attributes != original.attributes
        {
            return Err(AdmissionError::StaleEntry.into());
        }
        require_no_directory_streams(removable.file())?;
        removable.require_no_extended_attributes()?;
        Ok(())
    };
    verify_source()?;
    let snapshot = match &*destination {
        DirectoryMoveDestination::Created(created) => created.snapshot.clone(),
        DirectoryMoveDestination::Merged(snapshot) => snapshot.clone(),
    };
    let access = if matches!(&*destination, DirectoryMoveDestination::Created(_)) {
        DirectoryAccess::WriteDirectoryMetadata
    } else {
        DirectoryAccess::Observe
    };
    let held_destination =
        HeldParent::open_with_access(snapshot.path(), control.cancellation(), access)?;
    if held_destination.snapshot != snapshot {
        return Err(AdmissionError::StaleTarget.into());
    }
    if has_incomplete {
        return Ok(DirectorySourceOutcome::RetainedChildren {
            skipped: Vec::new(),
            incomplete: Vec::new(),
        });
    }
    let mut finalized_revision = None;
    if let DirectoryMoveDestination::Created(created) = destination {
        let file = held_destination.directory().file();
        let baseline = observe_entry_metadata(file)?;
        require_no_directory_streams(file)?;
        journal
            .record(Event::DirectoryMetadataIntent {
                entry: id,
                source: source.clone(),
                output: created.output.clone(),
            })
            .map_err(CopyFailure::Journal)?;
        let claim = loop {
            control.check()?;
            let Some(epoch) = control.admission_epoch()? else {
                continue;
            };
            verify_source()?;
            held_destination.check_chain(control.cancellation())?;
            if observe_entry_metadata(file)? != baseline {
                return Err(AdmissionError::StaleTarget.into());
            }
            require_no_directory_streams(file)?;
            if let Some(claim) = control.try_claim(epoch)? {
                break claim;
            }
        };
        let result = set_replace_basic_info(
            held_destination.directory().file(),
            FILE_BASIC_INFO {
                CreationTime: source.metadata().created,
                LastWriteTime: source.metadata().modified,
                ..Default::default()
            },
        );
        drop(claim);
        result?;
        let revision = observe_entry_metadata(held_destination.directory().file())?;
        if revision.created != source.metadata().created
            || revision.modified != source.metadata().modified
        {
            return Err(CopyFailure::UnsupportedMetadata);
        }
        created.times = DirectoryTimes::Preserved;
        finalized_revision = Some(revision);
        let mut chain = created.snapshot.chain.to_vec();
        if let Some(last) = chain.last_mut() {
            *last = directory_stamp(
                held_destination.directory(),
                created.output.parent.volume_serial_number(),
            )?;
        }
        created.snapshot.chain = chain.into();
        if let Err(error) = journal.record(Event::DirectoryMetadataObserved {
            entry: id,
            output: created.output.clone(),
            revision,
        }) {
            return Ok(DirectorySourceOutcome::ReceiptFailed {
                error: CopyFailure::Journal(error),
                removal: None,
            });
        }
    }
    if has_skipped {
        return Ok(DirectorySourceOutcome::RetainedChildren {
            skipped: Vec::new(),
            incomplete: Vec::new(),
        });
    }
    removing_source();
    let destination_snapshot = match destination {
        DirectoryMoveDestination::Created(created) => created.snapshot.clone(),
        DirectoryMoveDestination::Merged(snapshot) => snapshot.clone(),
    };
    let empty = || -> Result<bool, CopyFailure> {
        match enumerate_transfer_directory(removable.file(), &source.path(), 1, 1024, || {
            control.is_cancelled()
        }) {
            Ok(entries) => Ok(entries.is_empty()),
            Err(TransferEnumerationError::Limit) => Ok(false),
            Err(TransferEnumerationError::Cancelled) => Err(CopyFailure::Cancelled),
            Err(TransferEnumerationError::Io(error)) => {
                Err(CopyFailure::Native(io::Error::other(error)))
            }
        }
    };
    if !empty()? {
        return Ok(DirectorySourceOutcome::NonEmpty);
    }
    journal
        .record(Event::DirectoryRemoveIntent {
            entry: id,
            source: source.clone(),
            destination: destination_snapshot.clone(),
        })
        .map_err(CopyFailure::Journal)?;
    let claim = loop {
        control.check()?;
        let Some(epoch) = control.admission_epoch()? else {
            continue;
        };
        verify_source()?;
        let current_destination =
            HeldParent::open(destination_snapshot.path(), control.cancellation())?;
        if current_destination.snapshot != destination_snapshot {
            return Err(AdmissionError::StaleTarget.into());
        }
        if let Some(revision) = finalized_revision {
            let file = current_destination.directory().file();
            if observe_entry_metadata(file)? != revision {
                return Err(AdmissionError::StaleTarget.into());
            }
            require_no_directory_streams(file)?;
        }
        if !empty()? {
            return Ok(DirectorySourceOutcome::NonEmpty);
        }
        if let Some(claim) = control.try_claim(epoch)? {
            break claim;
        }
    };
    let result = removable.mark_delete_pending();
    drop(claim);
    drop(removable);
    let fresh = ScanCancellation::new();
    let after = source_parent.inspect_leaf(source.leaf(), &fresh);
    let observation = match after {
        Ok(TargetOccupancy::Vacant) if result.is_ok() => DirectoryRemovalObservation::Removed,
        Ok(TargetOccupancy::Occupied(other))
            if other.identity() != source.identity() && result.is_ok() =>
        {
            DirectoryRemovalObservation::Replaced(other.identity())
        }
        Ok(TargetOccupancy::Occupied(other)) if other.identity() == source.identity() => {
            DirectoryRemovalObservation::Retained
        }
        _ => DirectoryRemovalObservation::Uncertain,
    };
    if let Err(error) = journal.record(Event::DirectoryRemoveObserved {
        entry: id,
        source: source.clone(),
        destination: destination_snapshot,
        observation,
    }) {
        return Ok(DirectorySourceOutcome::ReceiptFailed {
            error: CopyFailure::Journal(error),
            removal: Some(observation),
        });
    }
    match (observation, result) {
        (DirectoryRemovalObservation::Removed, Ok(())) => {
            Ok(DirectorySourceOutcome::Removed { replacement: None })
        }
        (DirectoryRemovalObservation::Replaced(id), Ok(())) => {
            Ok(DirectorySourceOutcome::Removed {
                replacement: Some(id),
            })
        }
        (DirectoryRemovalObservation::Retained, Err(error))
            if error.kind() == io::ErrorKind::DirectoryNotEmpty =>
        {
            Ok(DirectorySourceOutcome::NonEmpty)
        }
        (DirectoryRemovalObservation::Retained, Err(error)) => {
            Ok(DirectorySourceOutcome::Failed(error.into()))
        }
        (_, Err(error)) => Ok(DirectorySourceOutcome::RemovalUncertain(error.into())),
        _ => Ok(DirectorySourceOutcome::RemovalUncertain(
            CopyFailure::ContentMismatch,
        )),
    }
}

/// Reject unsupported or stale source directories before admitting child work.
pub(super) fn check_directory_move_source_before_admission(
    source: &EntrySnapshot,
    control: &CopyControl,
) -> Result<(), CopyFailure> {
    control.check()?;
    let parent = HeldParent::open(source.parent().path(), control.cancellation())?;
    if parent.snapshot != *source.parent() {
        return Err(AdmissionError::StaleParent.into());
    }
    let held = parent
        .directory()
        .open_removable_directory_child(source.leaf().as_os_str(), source.identity())?;
    parent.check_chain(control.cancellation())?;
    if parent.capture_entry(held.file(), source.leaf())? != *source {
        return Err(AdmissionError::StaleEntry.into());
    }
    require_no_directory_streams(held.file())?;
    held.require_no_extended_attributes()?;
    control.check()
}
