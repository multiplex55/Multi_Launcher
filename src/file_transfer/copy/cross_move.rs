//! Verified cross-volume publication followed by journaled source retirement.

use super::*;
use crate::archive::transfer_native::TransferRelocation;
use crate::file_operations::native_file_identity;
use crate::file_transfer::TransferLeaf;
use crate::file_transfer::journal::{TransferJournal, TransferJournalEvent as Event};
use rand::TryRng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossMoveSourceState {
    Original,
    Quarantined,
    Removed,
    Uncertain,
}

#[derive(Clone, Debug)]
pub struct CrossMoveEffect {
    pub source: PathBuf,
    pub source_parent: NativeFileIdentity,
    pub source_identity: NativeFileIdentity,
    pub recovery: Option<PathBuf>,
    pub source_state: CrossMoveSourceState,
    pub output: CopyArtifact,
    pub verification: CopyVerification,
    pub restoration_error: Option<String>,
}

#[derive(Debug)]
pub struct CrossVolumeMovedFile {
    pub effect: CrossMoveEffect,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::file_transfer) enum CrossSourceAction {
    Relocate,
    Dispose,
    Restore,
}

pub(in crate::file_transfer) fn move_across_volumes(
    entry: &ApprovedTransferEntry,
    control: &CopyControl,
    progress: impl FnMut(u64),
    verification: impl FnOnce(bool),
    removing_source: impl FnOnce(),
    journal: &mut dyn TransferJournal,
) -> Result<CrossVolumeMovedFile, CopyError> {
    let ApprovedTarget::CreateAt(target) = entry.destination() else {
        return Err(CopyFailure::UnsupportedPlan.into());
    };
    let guarded = GuardedCopy::open_cross_volume(entry.source(), target, control)?;
    let lease = publish_vacant(
        guarded,
        true,
        control,
        progress,
        (|_| {}, || {}),
        verification,
        Some(journal),
    )?;
    let mut effect = CrossMoveEffect {
        source: entry.source().path(),
        source_parent: entry.source().parent().identity(),
        source_identity: entry.source().identity(),
        recovery: None,
        source_state: CrossMoveSourceState::Original,
        output: lease.copied.output.clone(),
        verification: lease.copied.verification,
        restoration_error: None,
    };
    removing_source();
    let retirement = retire_verified_source(lease, control, journal, &mut effect);
    if let Err(failure) = retirement {
        return Err(CopyError {
            failure,
            recovery_error: effect.restoration_error.clone(),
            artifact: Some(CopyRecoveryArtifact::CrossVolume(Box::new(effect))),
            replacement: None,
        });
    }
    Ok(CrossVolumeMovedFile {
        effect,
        bytes: entry
            .source()
            .metadata()
            .byte_len
            .ok_or(CopyFailure::UnsupportedPlan)?,
    })
}

fn record_source(
    journal: &mut dyn TransferJournal,
    effect: &CrossMoveEffect,
    action: CrossSourceAction,
    observed: bool,
) -> Result<(), CopyFailure> {
    let effect = effect.clone();
    let event = if observed {
        Event::CrossSourceObserved { action, effect }
    } else {
        Event::CrossSourceIntent { action, effect }
    };
    journal.record(event).map_err(CopyFailure::Journal)
}

fn retirement_name() -> Result<TransferLeaf, CopyFailure> {
    let random = rand::rngs::SysRng
        .try_next_u64()
        .map_err(|error| io::Error::other(error.to_string()))?;
    let leaf = format!(".multi-launcher-{random:016x}.move-source");
    TransferLeaf::new(std::ffi::OsStr::new(&leaf)).map_err(CopyFailure::from)
}

fn check_verified_output(
    lease: &VerifiedPublication<'_>,
    control: &CopyControl,
) -> Result<(), CopyFailure> {
    let file = lease.published.file();
    lease
        .copy
        .target_parent
        .check_chain(control.cancellation())?;
    checked_final_path(file, &lease.copied.output.path)?;
    if native_file_identity(file)? != lease.copied.output.identity
        || observe_entry_metadata(file)? != lease.revision
    {
        return Err(AdmissionError::StaleTarget.into());
    }
    if stream_inventory(file, lease.copy.length)? != lease.copy.streams() {
        return Err(CopyFailure::ContentMismatch);
    }
    recheck_named_zone(
        lease,
        &lease.copy.target_parent,
        lease.copy.target.requested_leaf(),
        lease.copied.output.identity,
        control,
    )?;
    if observe_entry_metadata(file)? != lease.revision {
        return Err(AdmissionError::StaleTarget.into());
    }
    Ok(())
}

fn recheck_named_zone(
    lease: &VerifiedPublication<'_>,
    parent: &HeldParent,
    leaf: &TransferLeaf,
    identity: NativeFileIdentity,
    control: &CopyControl,
) -> Result<(), CopyFailure> {
    let CopyVerification::Verified { zone_sha256, .. } = lease.copied.verification else {
        return Err(CopyFailure::ContentMismatch);
    };
    match (lease.copied.zone_bytes, zone_sha256) {
        (None, None) => Ok(()),
        (Some(length), Some(expected)) => {
            let named = parent
                .directory()
                .open_move_source_zone(leaf.as_os_str(), identity)?;
            if named.file().metadata()?.len() != length
                || hash_file(&named, length, control)? != expected
            {
                return Err(CopyFailure::ContentMismatch);
            }
            Ok(())
        }
        _ => Err(CopyFailure::ContentMismatch),
    }
}

fn check_source_payload(
    lease: &VerifiedPublication<'_>,
    leaf: &TransferLeaf,
    expected: EntryMetadata,
    control: &CopyControl,
) -> Result<(), CopyFailure> {
    let CopyVerification::Verified { sha256, .. } = lease.copied.verification else {
        return Err(CopyFailure::ContentMismatch);
    };
    let source = &lease.copy;
    check_source_name(lease, leaf, expected, control)?;
    if hash_file(&source.source_file, source.length, control)? != sha256 {
        return Err(CopyFailure::ContentMismatch);
    }
    recheck_named_zone(
        lease,
        &source.source_parent,
        leaf,
        source.source.identity(),
        control,
    )?;
    check_source_name(lease, leaf, expected, control)?;
    check_verified_output(lease, control)
}

fn check_source_name(
    lease: &VerifiedPublication<'_>,
    leaf: &TransferLeaf,
    expected: EntryMetadata,
    control: &CopyControl,
) -> Result<(), CopyFailure> {
    let source = &lease.copy;
    let relocation = source
        .source_relocation
        .as_ref()
        .ok_or(CopyFailure::UnsupportedPlan)?;
    source.source_parent.check_chain(control.cancellation())?;
    let current = source
        .source_parent
        .capture_entry(relocation.file(), leaf)?;
    if current.identity() != source.source.identity()
        || current.metadata() != expected
        || current.leaf() != leaf
    {
        return Err(AdmissionError::StaleEntry.into());
    }
    if stream_inventory(relocation.file(), source.length)? != source.streams() {
        return Err(CopyFailure::ContentMismatch);
    }
    relocation.require_no_extended_attributes()?;
    Ok(())
}

fn check_quarantine_revision(
    original: EntryMetadata,
    current: EntryMetadata,
) -> Result<EntryMetadata, CopyFailure> {
    if (EntryMetadata {
        changed: original.changed,
        ..current
    }) != original
    {
        return Err(AdmissionError::StaleEntry.into());
    }
    Ok(current)
}

fn inspect_exact_name(
    parent: &HeldParent,
    leaf: &TransferLeaf,
    expected: NativeFileIdentity,
    fresh: &ScanCancellation,
) -> Option<bool> {
    match parent.inspect_leaf(leaf, fresh) {
        Ok(TargetOccupancy::Vacant) => Some(false),
        Ok(TargetOccupancy::Occupied(entry)) => Some(entry.identity() == expected),
        Err(_) => None,
    }
}

fn retire_verified_source(
    mut lease: VerifiedPublication<'_>,
    control: &CopyControl,
    journal: &mut dyn TransferJournal,
    effect: &mut CrossMoveEffect,
) -> Result<(), CopyFailure> {
    let output_zone = lease.staged.zone.take();
    let zone = output_zone.as_ref().map(|zone| zone.length);
    drop(output_zone);
    // Reopen the currently named output stream after the exclusive staging
    // stream closes, while the exact base and destination parent stay held.
    let _named_destination_zone = if let Some(length) = zone {
        let CopyVerification::Verified {
            zone_sha256: Some(expected),
            ..
        } = lease.copied.verification
        else {
            return Err(CopyFailure::ContentMismatch);
        };
        let named = lease.copy.target_parent.directory().open_move_source_zone(
            lease.copy.target.requested_leaf().as_os_str(),
            lease.copied.output.identity,
        )?;
        if named.file().metadata()?.len() != length
            || hash_file(&named, length, control)? != expected
        {
            return Err(CopyFailure::ContentMismatch);
        }
        Some(named)
    } else {
        None
    };

    let source = &lease.copy;
    let relocation = source
        .source_relocation
        .as_ref()
        .ok_or(CopyFailure::UnsupportedPlan)?;
    let approved = source.source.metadata();
    let original = source.source.leaf();
    check_source_payload(&lease, original, approved, control)?;
    let quarantine = retirement_name()?;
    crate::file_transfer::check_target_bounds(source.source.parent(), &quarantine)?;
    let candidate = source.source.parent().path().join(quarantine.as_os_str());
    if crate::file_transfer::same_path(&candidate, &effect.source) {
        return Err(CopyFailure::DestinationOccupied);
    }
    effect.recovery = Some(candidate.clone());
    record_source(journal, effect, CrossSourceAction::Relocate, false)?;
    let claim = loop {
        control.check()?;
        let Some(epoch) = control.admission_epoch()? else {
            continue;
        };
        check_source_name(&lease, original, approved, control)?;
        check_verified_output(&lease, control)?;
        if let Some(claim) = control.try_claim(epoch)? {
            break claim;
        }
    };
    let renamed =
        relocation.rename_no_replace(source.source_parent.directory(), quarantine.as_os_str());
    drop(claim);
    let held_at_recovery = exact_held_leaf(relocation.file(), &candidate);
    if renamed.is_err() && exact_held_leaf(relocation.file(), &effect.source) {
        effect.recovery = None;
        return renamed.map_err(CopyFailure::from);
    }
    effect.source_state = if held_at_recovery {
        CrossMoveSourceState::Quarantined
    } else {
        CrossMoveSourceState::Uncertain
    };
    let observed_revision = if held_at_recovery {
        Some(observe_entry_metadata(relocation.file()))
    } else {
        None
    };
    record_source(journal, effect, CrossSourceAction::Relocate, true)?;
    if let Err(error) = renamed {
        restore_quarantined(source, relocation, journal, effect)?;
        return Err(error.into());
    }
    let prepared_disposition = (|| {
        let expected = check_quarantine_revision(
            approved,
            observed_revision.ok_or(CopyFailure::ContentMismatch)??,
        )?;
        check_source_payload(&lease, &quarantine, expected, control)?;
        record_source(journal, effect, CrossSourceAction::Dispose, false)?;
        loop {
            control.check()?;
            let Some(epoch) = control.admission_epoch()? else {
                continue;
            };
            recheck_named_zone(
                &lease,
                &source.source_parent,
                &quarantine,
                effect.source_identity,
                control,
            )?;
            check_source_name(&lease, &quarantine, expected, control)?;
            check_verified_output(&lease, control)?;
            if let Some(claim) = control.try_claim(epoch)? {
                return Ok::<_, CopyFailure>(claim);
            }
        }
    })();
    let claim = match prepared_disposition {
        Ok(claim) => claim,
        Err(error) => {
            if !matches!(error, CopyFailure::Journal(_)) {
                restore_quarantined(source, relocation, journal, effect)?;
            }
            return Err(error);
        }
    };
    let disposition = relocation.mark_delete_pending();
    drop(claim);

    let VerifiedPublication {
        copy,
        published,
        staged,
        revision,
        ..
    } = lease;
    let GuardedCopy {
        source_parent,
        source_file,
        source_zone,
        source_relocation,
        target_parent,
        source: snapshot,
        ..
    } = copy;
    drop(source_zone);
    drop(source_file);
    drop(source_relocation);
    let fresh = ScanCancellation::new();
    let still_at_original = inspect_exact_name(
        &source_parent,
        snapshot.leaf(),
        effect.source_identity,
        &fresh,
    );
    let still_at_recovery =
        inspect_exact_name(&source_parent, &quarantine, effect.source_identity, &fresh);
    effect.source_state = match (still_at_original, still_at_recovery, disposition.is_ok()) {
        (Some(true), Some(false), _) => CrossMoveSourceState::Original,
        (Some(false), Some(true), _) => CrossMoveSourceState::Quarantined,
        (Some(false), Some(false), true) => CrossMoveSourceState::Removed,
        _ => CrossMoveSourceState::Uncertain,
    };
    record_source(journal, effect, CrossSourceAction::Dispose, true)?;
    disposition?;
    if effect.source_state != CrossMoveSourceState::Removed {
        return Err(CopyFailure::ContentMismatch);
    }
    // The destination stays held through exact source observation and this
    // terminal check. A subsequent change remains an explicit partial result.
    target_parent.check_chain(&fresh)?;
    checked_final_path(published.file(), &effect.output.path)?;
    if native_file_identity(published.file())? != effect.output.identity
        || observe_entry_metadata(published.file())? != revision
    {
        return Err(AdmissionError::StaleTarget.into());
    }
    drop(staged);
    Ok(())
}

fn restore_quarantined(
    copy: &GuardedCopy<'_>,
    relocation: &TransferRelocation,
    journal: &mut dyn TransferJournal,
    effect: &mut CrossMoveEffect,
) -> Result<(), CopyFailure> {
    let Some(candidate) = effect.recovery.clone() else {
        return Ok(());
    };
    if effect.source_state != CrossMoveSourceState::Quarantined {
        return Ok(());
    }
    let fresh = ScanCancellation::new();
    let source = copy.source.leaf();
    let can_restore = || {
        exact_held_leaf(relocation.file(), &candidate)
            && matches!(
                copy.source_parent.inspect_leaf(source, &fresh),
                Ok(TargetOccupancy::Vacant)
            )
    };
    if !can_restore() {
        return Ok(());
    }
    record_source(journal, effect, CrossSourceAction::Restore, false)?;
    if !can_restore() {
        return Ok(());
    }
    let repaired = relocation.rename_no_replace(copy.source_parent.directory(), source.as_os_str());
    if exact_held_leaf(relocation.file(), &effect.source) {
        effect.source_state = CrossMoveSourceState::Original;
    } else if !exact_held_leaf(relocation.file(), &candidate) {
        effect.source_state = CrossMoveSourceState::Uncertain;
    }
    record_source(journal, effect, CrossSourceAction::Restore, true)?;
    if let Err(error) = repaired {
        effect.restoration_error = Some(error.to_string());
    }
    Ok(())
}
