//! Journaled, same-volume rename of a held ordinary file.
use super::copy::{exact_held_leaf, hash_file, stream_inventory};
use super::execute::ResolvedTransferTarget;
use super::journal::{TransferJournal, TransferJournalEvent as Event};
use super::{
    AdmissionError, CopyControl, CopyError, CopyFailure, CopyRecoveryArtifact, CopyVerification,
    DirectoryAccess, EntryKind, EntryMetadata, HeldParent, TargetOccupancy,
};
use crate::file_operations::{NativeFileIdentity, ScanCancellation, set_replace_basic_info};
use std::path::PathBuf;
use windows::Win32::Storage::FileSystem::FILE_BASIC_INFO;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveLocation {
    Source,
    Destination,
    Uncertain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveSourceDisposition {
    Vacant,
    Foreign(NativeFileIdentity),
    Unknown,
}
#[derive(Clone, Debug)]
pub struct MoveEffect {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_parent: NativeFileIdentity,
    pub destination_parent: NativeFileIdentity,
    pub identity: NativeFileIdentity,
    pub original: EntryMetadata,
    pub location: MoveLocation,
    pub source_disposition: MoveSourceDisposition,
    pub final_metadata: Option<EntryMetadata>,
    pub verification: Option<CopyVerification>,
}
#[derive(Debug)]
pub struct MovedFile {
    pub effect: MoveEffect,
    pub bytes: u64,
}

fn after_effect(failure: CopyFailure, effect: &MoveEffect) -> CopyError {
    CopyError {
        failure,
        artifact: Some(CopyRecoveryArtifact::Relocation(Box::new(effect.clone()))),
        recovery_error: None,
        replacement: None,
    }
}

pub(super) fn move_same_volume_file(
    bound: &ResolvedTransferTarget<'_>,
    control: &CopyControl,
    verify_content: bool,
    mut progress: impl FnMut(bool),
    journal: &mut dyn TransferJournal,
) -> Result<MovedFile, CopyError> {
    let target = bound.target();
    let source = bound.source();
    let undo_digest = bound.undo_digest();
    if undo_digest.is_some() && !verify_content {
        return Err(CopyFailure::UnsupportedPlan.into());
    }
    if bound.replacing()
        || !matches!(target.occupancy(), TargetOccupancy::Vacant)
        || source.metadata().kind != EntryKind::File
    {
        return Err(CopyFailure::UnsupportedPlan.into());
    }
    let length = source
        .metadata()
        .byte_len
        .ok_or(CopyFailure::UnsupportedPlan)?;
    control.check()?;
    let source_parent = HeldParent::open(source.parent().path(), control.cancellation())
        .map_err(CopyFailure::from)?;
    let destination_parent = HeldParent::open_with_access(
        target.parent().path(),
        control.cancellation(),
        DirectoryAccess::CreateFile,
    )
    .map_err(CopyFailure::from)?;
    if source_parent.snapshot != *source.parent() || destination_parent.snapshot != *target.parent()
    {
        return Err(CopyFailure::Admission(AdmissionError::StaleParent).into());
    }
    if source_parent.snapshot.identity().volume_serial_number()
        != destination_parent
            .snapshot
            .identity()
            .volume_serial_number()
    {
        return Err(CopyFailure::UnsupportedPlan.into());
    }
    let held = source_parent
        .directory()
        .open_move_relocation(source.leaf().as_os_str(), source.identity(), verify_content)
        .map_err(CopyFailure::from)?;
    let source_path = source.path();
    let destination = target
        .parent()
        .path()
        .join(target.requested_leaf().as_os_str());
    let mut effect = MoveEffect {
        source: source_path.clone(),
        destination: destination.clone(),
        source_parent: source.parent().identity(),
        destination_parent: target.parent().identity(),
        identity: held.identity(),
        original: source.metadata(),
        location: MoveLocation::Source,
        source_disposition: MoveSourceDisposition::Unknown,
        final_metadata: None,
        verification: None,
    };
    let check_before = || -> Result<(), CopyFailure> {
        source_parent.check_chain(control.cancellation())?;
        destination_parent.check_chain(control.cancellation())?;
        if source_parent.capture_entry(held.file(), source.leaf())? != *source
            || !exact_held_leaf(held.file(), &source_path)
        {
            return Err(AdmissionError::StaleEntry.into());
        }
        if !matches!(
            destination_parent.inspect_leaf(target.requested_leaf(), control.cancellation())?,
            TargetOccupancy::Vacant
        ) {
            return Err(CopyFailure::DestinationOccupied);
        }
        Ok(())
    };
    check_before()?;
    // Unknown streams are deliberately left untouched when verification is off.
    let inventory = if verify_content {
        Some(stream_inventory(held.file(), length)?)
    } else {
        None
    };
    let zone = match inventory.and_then(|v| v.zone) {
        Some(zone_length) => Some((
            source_parent
                .directory()
                .open_move_source_zone(source.leaf().as_os_str(), source.identity())
                .map_err(CopyFailure::from)?,
            zone_length,
        )),
        None => None,
    };
    let expected = if verify_content {
        progress(true);
        let data = held.data_file().ok_or(CopyFailure::UnsupportedPlan)?;
        let data_hash = hash_file(data, length, control)?;
        let zone_hash = zone
            .as_ref()
            .map(|(file, len)| hash_file(file, *len, control))
            .transpose()?;
        Some((data_hash, zone_hash))
    } else {
        None
    };
    if let Some(undo) = undo_digest
        && expected != Some((undo.sha256, undo.zone_sha256))
    {
        return Err(CopyFailure::ContentMismatch.into());
    }
    journal
        .record(Event::MoveRenameIntent {
            source: source_path.clone(),
            target: destination.clone(),
            source_parent: effect.source_parent,
            target_parent: effect.destination_parent,
            identity: effect.identity,
            original: effect.original,
        })
        .map_err(CopyFailure::Journal)?;
    let claim = loop {
        control.check()?;
        let Some(epoch) = control.admission_epoch()? else {
            continue;
        };
        check_before()?;
        if let Some(expected_inventory) = inventory
            && stream_inventory(held.file(), length)? != expected_inventory
        {
            return Err(CopyFailure::Admission(AdmissionError::StaleEntry).into());
        }
        if let Some(undo) = undo_digest {
            let data = held.data_file().ok_or(CopyFailure::UnsupportedPlan)?;
            let hash = hash_file(data, length, control)?;
            let zone_hash = match inventory.and_then(|v| v.zone) {
                Some(zone_length) => {
                    let named = source_parent
                        .directory()
                        .open_move_source_zone(source.leaf().as_os_str(), source.identity())
                        .map_err(CopyFailure::from)?;
                    if named.file().metadata().map_err(CopyFailure::from)?.len() != zone_length {
                        return Err(CopyFailure::ContentMismatch.into());
                    }
                    Some(hash_file(&named, zone_length, control)?)
                }
                None => None,
            };
            if hash != undo.sha256 || zone_hash != undo.zone_sha256 {
                return Err(CopyFailure::ContentMismatch.into());
            }
            check_before()?;
            if stream_inventory(held.file(), length)?
                != inventory.ok_or(CopyFailure::UnsupportedPlan)?
            {
                return Err(CopyFailure::Admission(AdmissionError::StaleEntry).into());
            }
        }
        if let Some(claim) = control.try_claim(epoch)? {
            break claim;
        }
    };
    let renamed = held.rename_no_replace(
        destination_parent.directory(),
        target.requested_leaf().as_os_str(),
    );
    drop(claim);
    effect.location = if exact_held_leaf(held.file(), &destination) {
        MoveLocation::Destination
    } else if renamed.is_err() && exact_held_leaf(held.file(), &source_path) {
        MoveLocation::Source
    } else {
        MoveLocation::Uncertain
    };
    if effect.location == MoveLocation::Source {
        let failure = match renamed {
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                CopyFailure::DestinationOccupied
            }
            Err(error) => error.into(),
            Ok(()) => CopyFailure::ContentMismatch,
        };
        return Err(failure.into());
    }
    let observed_metadata = if effect.location == MoveLocation::Destination {
        super::observe_entry_metadata(held.file()).map(Some)
    } else {
        Ok(None)
    };
    effect.final_metadata = observed_metadata.as_ref().ok().copied().flatten();
    let fresh = ScanCancellation::new();
    effect.source_disposition = match source_parent.inspect_leaf(source.leaf(), &fresh) {
        Ok(TargetOccupancy::Vacant) => MoveSourceDisposition::Vacant,
        Ok(TargetOccupancy::Occupied(other)) if other.identity() != effect.identity => {
            MoveSourceDisposition::Foreign(other.identity())
        }
        _ => MoveSourceDisposition::Unknown,
    };
    if let Err(error) = journal.record(Event::MoveRenameObserved {
        effect: effect.clone(),
    }) {
        return Err(after_effect(CopyFailure::Journal(error), &effect));
    }
    if let Err(error) = renamed {
        return Err(after_effect(error.into(), &effect));
    }
    if effect.location != MoveLocation::Destination
        || effect.source_disposition == MoveSourceDisposition::Unknown
    {
        return Err(after_effect(CopyFailure::ContentMismatch, &effect));
    }
    progress(verify_content);
    let finish = (|| -> Result<(), CopyFailure> {
        let observed_metadata = observed_metadata?.ok_or(CopyFailure::ContentMismatch)?;
        let mut metadata_intent_written = false;
        loop {
            control.check()?;
            let Some(epoch) = control.admission_epoch()? else {
                continue;
            };
            destination_parent.check_chain(control.cancellation())?;
            if !exact_held_leaf(held.file(), &destination) {
                return Err(AdmissionError::StaleEntry.into());
            }
            let current = super::observe_entry_metadata(held.file())?;
            if current != observed_metadata {
                return Err(AdmissionError::StaleEntry.into());
            }
            if current.modified != effect.original.modified {
                return Err(CopyFailure::UnsupportedMetadata);
            }
            if current.created == effect.original.created {
                break;
            }
            if !metadata_intent_written {
                journal
                    .record(Event::MoveMetadataIntent {
                        effect: effect.clone(),
                    })
                    .map_err(CopyFailure::Journal)?;
                metadata_intent_written = true;
                // The receipt write can take time. Acquire fresh evidence
                // before the metadata mutation, even without a pause.
                continue;
            }
            if let Some(claim) = control.try_claim(epoch)? {
                let result = set_replace_basic_info(
                    held.file(),
                    FILE_BASIC_INFO {
                        CreationTime: effect.original.created,
                        ..Default::default()
                    },
                );
                drop(claim);
                result?;
                effect.final_metadata = Some(super::observe_entry_metadata(held.file())?);
                journal
                    .record(Event::MoveMetadataObserved {
                        effect: effect.clone(),
                    })
                    .map_err(CopyFailure::Journal)?;
                break;
            }
        }
        let current = effect.final_metadata.ok_or(CopyFailure::ContentMismatch)?;
        if super::observe_entry_metadata(held.file())? != current {
            return Err(AdmissionError::StaleEntry.into());
        }
        if current.kind != EntryKind::File
            || current.byte_len != effect.original.byte_len
            || current.attributes != effect.original.attributes
            || current.created != effect.original.created
            || current.modified != effect.original.modified
        {
            return Err(CopyFailure::ContentMismatch);
        }
        effect.final_metadata = Some(current);
        if let Some((data_hash, zone_hash)) = expected {
            if stream_inventory(held.file(), length)?
                != inventory.ok_or(CopyFailure::UnsupportedPlan)?
            {
                return Err(CopyFailure::ContentMismatch);
            }
            let actual_data = hash_file(
                held.data_file().ok_or(CopyFailure::UnsupportedPlan)?,
                length,
                control,
            )?;
            let actual_zone = zone
                .as_ref()
                .map(|(_, len)| {
                    let named_zone = destination_parent.directory().open_move_source_zone(
                        target.requested_leaf().as_os_str(),
                        effect.identity,
                    )?;
                    if named_zone.file().metadata()?.len() != *len {
                        return Err(CopyFailure::ContentMismatch);
                    }
                    hash_file(&named_zone, *len, control)
                })
                .transpose()?;
            if actual_data != data_hash
                || actual_zone != zone_hash
                || stream_inventory(held.file(), length)?
                    != inventory.ok_or(CopyFailure::UnsupportedPlan)?
            {
                return Err(CopyFailure::ContentMismatch);
            }
            effect.verification = Some(CopyVerification::Verified {
                sha256: actual_data,
                zone_sha256: actual_zone,
            });
        } else {
            effect.verification = Some(CopyVerification::NotRequested);
        }
        destination_parent.check_chain(control.cancellation())?;
        if !exact_held_leaf(held.file(), &destination)
            || super::observe_entry_metadata(held.file())? != current
        {
            return Err(AdmissionError::StaleEntry.into());
        }
        control.check()?;
        journal
            .record(Event::MoveVerified {
                effect: effect.clone(),
            })
            .map_err(CopyFailure::Journal)?;
        Ok(())
    })();
    finish.map_err(|failure| after_effect(failure, &effect))?;
    Ok(MovedFile {
        effect,
        bytes: length,
    })
}
