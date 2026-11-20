//! Explicit, read-only preparation of fresh recovery reviews.
use super::recovery::{ReceiptEntryStatus, ReceiptHealth, ReceiptHistory};
use super::wire::{self, Effect, PlannedTarget, Record, SemanticPrefix, TargetPolicy};
use crate::file_operations::ScanCancellation;
use crate::file_transfer::{
    ApprovedTransferPlan, ParentSnapshot, TargetOccupancy, TransferAction, TransferLeaf,
    TransferReceiptId, TransferReview, inspect_parent, inspect_source, inspect_target,
};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryActionError {
    Unavailable,
    Cancelled,
    NotEligible,
    Changed,
    Occupied,
    Review,
}
impl std::fmt::Display for RecoveryActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Receipt is unavailable.",
            Self::Cancelled => "Recovery review was cancelled.",
            Self::NotEligible => "This item needs a new ordinary review.",
            Self::Changed => "The recorded source or parent changed.",
            Self::Occupied => "The recorded destination is occupied.",
            Self::Review => "Could not prepare the exact recovery review.",
        })
    }
}
impl std::error::Error for RecoveryActionError {}

pub struct PreparedReceiptRetry {
    review: TransferReview,
    source: PathBuf,
    destination: PathBuf,
    origin: crate::file_transfer::plan::RecoveryOrigin,
}
impl PreparedReceiptRetry {
    pub fn source(&self) -> &Path {
        &self.source
    }
    pub fn destination(&self) -> &Path {
        &self.destination
    }
    pub fn approve(
        self,
        cancel: &ScanCancellation,
    ) -> Result<ApprovedTransferPlan, RecoveryActionError> {
        let mut plan = self
            .review
            .approve(self.review.generation(), cancel)
            .map_err(|_| {
                if cancel.is_cancelled() {
                    RecoveryActionError::Cancelled
                } else {
                    RecoveryActionError::Changed
                }
            })?;
        plan.set_recovery_origin(self.origin);
        Ok(plan)
    }
}
pub struct PreparedReceiptUndo {
    review: TransferReview,
    source: PathBuf,
    destination: PathBuf,
    digest: crate::file_transfer::plan::UndoDigest,
    origin: crate::file_transfer::plan::RecoveryOrigin,
}
impl PreparedReceiptUndo {
    pub fn source(&self) -> &Path {
        &self.source
    }
    pub fn destination(&self) -> &Path {
        &self.destination
    }
    pub fn approve(
        self,
        cancel: &ScanCancellation,
    ) -> Result<ApprovedTransferPlan, RecoveryActionError> {
        let mut plan = self
            .review
            .approve(self.review.generation(), cancel)
            .map_err(|_| {
                if cancel.is_cancelled() {
                    RecoveryActionError::Cancelled
                } else {
                    RecoveryActionError::Changed
                }
            })?;
        plan.seal_undo(self.digest)
            .map_err(|_| RecoveryActionError::Review)?;
        plan.set_recovery_origin(self.origin);
        Ok(plan)
    }
}

impl ReceiptHistory<'_> {
    pub fn prepare_retry(
        &self,
        receipt: TransferReceiptId,
        node: u32,
        cancel: &ScanCancellation,
    ) -> Result<PreparedReceiptRetry, RecoveryActionError> {
        let entry = self
            .store()
            .read_selected(wire::ReceiptId(receipt.0))
            .map_err(|_| RecoveryActionError::Unavailable)?
            .ok_or(RecoveryActionError::Unavailable)?;
        let detail = super::recovery::reconcile_entry(&entry, cancel).map_err(|_| {
            if cancel.is_cancelled() {
                RecoveryActionError::Cancelled
            } else {
                RecoveryActionError::Unavailable
            }
        })?;
        if !detail.trusted_prefix || !detail.outline_complete {
            return Err(RecoveryActionError::NotEligible);
        }
        if entry.prefix.records.iter().any(|r| {
            matches!(
                r,
                Record::RecoveryOrigin {
                    verb: wire::RecoveryVerb::Undo,
                    ..
                }
            )
        }) {
            return Err(RecoveryActionError::NotEligible);
        }
        let (origin_receipt, origin_node) =
            match entry.prefix.records.iter().find_map(|record| match record {
                Record::RecoveryOrigin {
                    id,
                    node,
                    verb: wire::RecoveryVerb::Retry,
                } => Some((TransferReceiptId(id.0), *node)),
                _ => None,
            }) {
                Some(origin) if node == 0 => origin,
                Some(_) => return Err(RecoveryActionError::NotEligible),
                None => (receipt, node),
            };
        let row = detail
            .entries
            .iter()
            .find(|e| e.index == node)
            .ok_or(RecoveryActionError::NotEligible)?;
        if !matches!(
            row.status,
            ReceiptEntryStatus::Remaining
                | ReceiptEntryStatus::Failed
                | ReceiptEntryStatus::Cancelled
                | ReceiptEntryStatus::Blocked
        ) {
            return Err(RecoveryActionError::NotEligible);
        }
        if entry
            .prefix
            .records
            .iter()
            .any(|r| matches!(r, Record::Effect { node: id, .. } if id.0 == node))
        {
            return Err(RecoveryActionError::NotEligible);
        }
        let outline = entry
            .prefix
            .nodes
            .get(node as usize)
            .ok_or(RecoveryActionError::NotEligible)?;
        let object = outline
            .source
            .as_ref()
            .ok_or(RecoveryActionError::NotEligible)?;
        let revision = outline.revision.ok_or(RecoveryActionError::NotEligible)?;
        if revision.kind != wire::Kind::File {
            return Err(RecoveryActionError::NotEligible);
        }
        let old_parent = entry
            .prefix
            .parent_snapshot(object.place.parent)
            .map_err(|_| RecoveryActionError::Unavailable)?;
        let current_parent = verify_parent(&old_parent, cancel)?;
        let source_path = current_parent
            .path()
            .join(OsString::from_wide(&object.place.leaf.0));
        let source =
            inspect_source(&source_path, cancel).map_err(|_| RecoveryActionError::Changed)?;
        if source.identity() != object.identity
            || source.metadata() != revision.into()
            || source.parent() != &current_parent
            || source.leaf().as_os_str() != OsString::from_wide(&object.place.leaf.0)
        {
            return Err(RecoveryActionError::Changed);
        }
        let (old_target_parent, leaf) = target_for(&entry.prefix, outline, cancel)?;
        let target_parent = verify_parent(&old_target_parent, cancel)?;
        let leaf = TransferLeaf::new(&leaf).map_err(|_| RecoveryActionError::NotEligible)?;
        let target = inspect_target(&target_parent, &leaf, cancel)
            .map_err(|_| RecoveryActionError::Changed)?;
        if !matches!(target.occupancy(), TargetOccupancy::Vacant) {
            return Err(RecoveryActionError::Occupied);
        }
        let (action, verify) = match entry.prefix.records.first() {
            Some(Record::Header {
                action,
                verification_requested,
                ..
            }) => (
                match action {
                    wire::Action::Copy => {
                        TransferAction::CopyTo(target_parent.path().to_path_buf())
                    }
                    wire::Action::Move => {
                        TransferAction::MoveTo(target_parent.path().to_path_buf())
                    }
                    wire::Action::Duplicate => TransferAction::Duplicate,
                },
                *verification_requested,
            ),
            _ => return Err(RecoveryActionError::NotEligible),
        };
        let destination = target_parent.path().join(leaf.as_os_str());
        let review =
            crate::file_transfer::plan::review_exact_file(source, target, action, verify, cancel)
                .map_err(|_| RecoveryActionError::Review)?;
        Ok(PreparedReceiptRetry {
            review,
            source: source_path,
            destination,
            origin: crate::file_transfer::plan::RecoveryOrigin {
                receipt: origin_receipt,
                node: origin_node,
                verb: crate::file_transfer::plan::RecoveryVerb::Retry,
            },
        })
    }
    pub fn prepare_undo(
        &self,
        receipt: TransferReceiptId,
        node: u32,
        cancel: &ScanCancellation,
    ) -> Result<PreparedReceiptUndo, RecoveryActionError> {
        let detail = self.reconcile(receipt, cancel).map_err(|_| {
            if cancel.is_cancelled() {
                RecoveryActionError::Cancelled
            } else {
                RecoveryActionError::Unavailable
            }
        })?;
        if !detail.trusted_prefix
            || !detail.outline_complete
            || detail.summary.health != ReceiptHealth::Completed
        {
            return Err(RecoveryActionError::NotEligible);
        }
        let evidence = detail
            .entries
            .iter()
            .find(|e| e.index == node)
            .and_then(|e| e.verified_atomic_move.as_ref())
            .ok_or(RecoveryActionError::NotEligible)?;
        let output_parent = verify_parent(&evidence.output_parent, cancel)?;
        let original_parent = verify_parent(&evidence.source_parent, cancel)?;
        let output =
            inspect_source(&evidence.output, cancel).map_err(|_| RecoveryActionError::Changed)?;
        if output.metadata() != evidence.output_revision
            || output.identity() != evidence.output_identity
            || output.parent() != &output_parent
            || output.leaf().as_os_str()
                != evidence
                    .output
                    .file_name()
                    .ok_or(RecoveryActionError::Changed)?
        {
            return Err(RecoveryActionError::Changed);
        }
        let original_leaf = evidence
            .source
            .file_name()
            .ok_or(RecoveryActionError::NotEligible)?;
        let original_leaf =
            TransferLeaf::new(original_leaf).map_err(|_| RecoveryActionError::NotEligible)?;
        let target = inspect_target(&original_parent, &original_leaf, cancel)
            .map_err(|_| RecoveryActionError::Changed)?;
        if !matches!(target.occupancy(), TargetOccupancy::Vacant) {
            return Err(RecoveryActionError::Occupied);
        }
        let review = crate::file_transfer::plan::review_exact_file(
            output,
            target,
            TransferAction::MoveTo(original_parent.path().to_path_buf()),
            true,
            cancel,
        )
        .map_err(|_| RecoveryActionError::Review)?;
        Ok(PreparedReceiptUndo {
            review,
            source: evidence.output.clone(),
            destination: evidence.source.clone(),
            digest: crate::file_transfer::plan::UndoDigest {
                sha256: evidence.sha256,
                zone_sha256: evidence.zone_sha256,
            },
            origin: crate::file_transfer::plan::RecoveryOrigin {
                receipt,
                node,
                verb: crate::file_transfer::plan::RecoveryVerb::Undo,
            },
        })
    }
}

fn verify_parent(
    expected: &ParentSnapshot,
    cancel: &ScanCancellation,
) -> Result<ParentSnapshot, RecoveryActionError> {
    if cancel.is_cancelled() {
        return Err(RecoveryActionError::Cancelled);
    }
    let current =
        inspect_parent(expected.path(), cancel).map_err(|_| RecoveryActionError::Changed)?;
    let (a_root, a_parts) =
        crate::archive::transfer_native::split_local_absolute_path(current.path())
            .map_err(|_| RecoveryActionError::Changed)?;
    let (b_root, b_parts) =
        crate::archive::transfer_native::split_local_absolute_path(expected.path())
            .map_err(|_| RecoveryActionError::Changed)?;
    if current.chain != expected.chain || a_root != b_root || a_parts != b_parts {
        return Err(RecoveryActionError::Changed);
    }
    Ok(current)
}

fn target_for(
    prefix: &SemanticPrefix,
    outline: &wire::OutlineNode,
    _cancel: &ScanCancellation,
) -> Result<(ParentSnapshot, OsString), RecoveryActionError> {
    match &outline.target {
        PlannedTarget::Direct {
            place,
            policy: TargetPolicy::Vacant,
        } => Ok((
            prefix
                .parent_snapshot(place.parent)
                .map_err(|_| RecoveryActionError::Unavailable)?,
            OsString::from_wide(&place.leaf.0),
        )),
        PlannedTarget::Child { parent, leaf } => {
            let directory = prefix.records.iter().find_map(|record| match record {
                Record::Effect {
                    node,
                    effect: Effect::DirectoryCreated { parent_proof, .. },
                } if node == parent => Some(*parent_proof),
                _ => None,
            });
            let target_parent = if let Some(id) = directory {
                prefix
                    .parent_snapshot(id)
                    .map_err(|_| RecoveryActionError::Unavailable)?
            } else {
                let parent_node = prefix
                    .nodes
                    .get(parent.0 as usize)
                    .ok_or(RecoveryActionError::NotEligible)?;
                let PlannedTarget::Direct {
                    policy:
                        TargetPolicy::Merge {
                            incumbent,
                            revision,
                        },
                    ..
                } = &parent_node.target
                else {
                    return Err(RecoveryActionError::NotEligible);
                };
                if !prefix.records.iter().any(|record| {
                    matches!(record,
                    Record::Bind(binding)
                        if binding.node == *parent
                            && binding.incumbent.as_ref() == Some(incumbent)
                            && binding.incumbent_revision == Some(*revision))
                }) {
                    return Err(RecoveryActionError::NotEligible);
                }
                let base = prefix
                    .parent_snapshot(incumbent.place.parent)
                    .map_err(|_| RecoveryActionError::Unavailable)?;
                let mut path = base.path().to_path_buf();
                path.push(OsString::from_wide(&incumbent.place.leaf.0));
                let mut chain = base.chain.to_vec();
                chain.push(crate::file_transfer::DirectoryStamp {
                    identity: incumbent.identity,
                    created: revision.created,
                });
                ParentSnapshot {
                    path: path.into(),
                    chain: chain.into(),
                }
            };
            Ok((target_parent, OsString::from_wide(&leaf.0)))
        }
        _ => Err(RecoveryActionError::NotEligible),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_transfer::receipts::store::ReceiptStore;
    use crate::file_transfer::{
        CopyControl, ExecutionProgress, TransferDraft, inspect_source_tree, review_transfer,
    };
    use crate::platform::app_data::AppDataRoot;
    use std::{fs, sync::Arc};
    fn zone(path: &Path) -> PathBuf {
        let mut name = path.as_os_str().to_os_string();
        name.push(":Zone.Identifier:$DATA");
        name.into()
    }
    #[test]
    fn receipt_actions_retry_only_untouched_row_with_exact_target() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let a = temp.path().join("a.txt");
        let b = temp.path().join("b.txt");
        let out = temp.path().join("out");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        fs::create_dir(&out).unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[a.clone(), b.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(out.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut store = ReceiptStore::new(&root);
        let mut session = store.admit(&plan).unwrap();
        let receipt = TransferReceiptId(session.receipt_id());
        let control = CopyControl::default();
        let report = crate::file_transfer::execute::execute_with_journal(
            &plan,
            &control,
            |event| {
                if matches!(event, ExecutionProgress::FileCompleted { .. }) {
                    control.cancel();
                }
            },
            &mut session,
        )
        .unwrap();
        crate::file_transfer::jobs::finish_receipt(
            &mut session,
            &report,
            crate::file_transfer::TransferJobStatus::Partial,
        )
        .unwrap();
        drop(session);
        let history = ReceiptHistory::from_store(&store);
        assert!(matches!(
            history.prepare_retry(receipt, 0, &cancel),
            Err(RecoveryActionError::NotEligible)
        ));
        fs::remove_file(out.join("a.txt")).unwrap();
        assert!(matches!(
            history.prepare_retry(receipt, 0, &cancel),
            Err(RecoveryActionError::NotEligible)
        ));
        fs::write(out.join("a.txt"), b"a").unwrap();
        fs::write(out.join("b.txt"), b"foreign").unwrap();
        assert!(matches!(
            history.prepare_retry(receipt, 1, &cancel),
            Err(RecoveryActionError::Occupied)
        ));
        fs::remove_file(out.join("b.txt")).unwrap();
        let retry = history.prepare_retry(receipt, 1, &cancel).unwrap();
        assert!(retry.destination().ends_with("b.txt"));
        let fresh = retry.approve(&cancel).unwrap();
        let unknown = temp
            .path()
            .join("transfers")
            .join("receipts")
            .join("abababababababababababababababab.receipt");
        fs::write(&unknown, b"unknown receipt origin").unwrap();
        assert!(matches!(
            store.admit(&fresh),
            Err(super::super::store::ReceiptStoreError::InvalidRecord)
        ));
        assert!(!out.join("b.txt").exists());
        assert_eq!(fs::read(&unknown).unwrap(), b"unknown receipt origin");
        fs::remove_file(&unknown).unwrap();
        let mut new_session = store.admit(&fresh).unwrap();
        let report = crate::file_transfer::execute::execute_with_journal(
            &fresh,
            &CopyControl::default(),
            |_| {},
            &mut new_session,
        )
        .unwrap();
        assert!(matches!(
            report.rows[0].state,
            crate::file_transfer::ExecutionState::Copied(_)
        ));
        assert_eq!(fs::read(out.join("a.txt")).unwrap(), b"a");
        assert_eq!(fs::read(out.join("b.txt")).unwrap(), b"b");
        new_session
            .finish_entry(
                fresh.source().entries()[0].id(),
                wire::EntryOutcome::Completed,
                wire::Disposition::Settled,
            )
            .unwrap();
        new_session.finish_job(wire::JobOutcome::Completed).unwrap();
        drop(new_session);
        fs::remove_file(out.join("b.txt")).unwrap();
        let history = ReceiptHistory::from_store(&store);
        let repeated = history
            .prepare_retry(receipt, 1, &cancel)
            .unwrap()
            .approve(&cancel)
            .unwrap();
        assert!(matches!(
            store.admit(&repeated),
            Err(super::super::store::ReceiptStoreError::InvalidRecord)
        ));
        assert_eq!(fs::read(&b).unwrap(), b"b");
        assert!(!out.join("b.txt").exists());
        assert!(
            ReceiptHistory::from_store(&store)
                .reconcile(receipt, &cancel)
                .is_ok()
        );
    }
    #[test]
    fn receipt_actions_retry_uses_proven_created_parent() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("folder");
        let out = temp.path().join("out");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir(&out).unwrap();
        fs::write(source.join("nested/a"), b"a").unwrap();
        fs::write(source.join("nested/b"), b"b").unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(out.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let selected = review
            .source()
            .entries()
            .iter()
            .find(|entry| entry.path().ends_with("b"))
            .unwrap()
            .id()
            .index() as u32;
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut store = ReceiptStore::new(&root);
        let mut session = store.admit(&plan).unwrap();
        let id = TransferReceiptId(session.receipt_id());
        let control = CopyControl::default();
        let report = crate::file_transfer::execute::execute_with_journal(
            &plan,
            &control,
            |event| {
                if matches!(event, ExecutionProgress::FileCompleted { .. }) {
                    control.cancel();
                }
            },
            &mut session,
        )
        .unwrap();
        crate::file_transfer::jobs::finish_receipt(
            &mut session,
            &report,
            crate::file_transfer::TransferJobStatus::Partial,
        )
        .unwrap();
        drop(session);
        let history = ReceiptHistory::from_store(&store);
        let retry = history.prepare_retry(id, selected, &cancel).unwrap();
        assert!(retry.destination().ends_with("folder/nested/b"));
        let fresh = retry.approve(&cancel).unwrap();
        let mut session = store.admit(&fresh).unwrap();
        let report = crate::file_transfer::execute::execute_with_journal(
            &fresh,
            &CopyControl::default(),
            |_| {},
            &mut session,
        )
        .unwrap();
        assert!(matches!(
            report.rows[0].state,
            crate::file_transfer::ExecutionState::Copied(_)
        ));
        assert_eq!(fs::read(out.join("folder/nested/b")).unwrap(), b"b");
    }
    #[test]
    fn receipt_actions_verified_atomic_undo_reuses_identity_and_refuses_foreign_original() {
        use crate::activity::ActivityCenter;
        use crate::file_transfer::{TransferJobStatus, TransferJobs};
        use std::time::{Duration, Instant};
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("original.txt");
        let out = temp.path().join("out");
        fs::write(&source, b"content").unwrap();
        let zone_data = b"[ZoneTransfer]\r\nZoneId=3\r\n";
        fs::write(zone(&source), zone_data).unwrap();
        fs::create_dir(&out).unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::MoveTo(out.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let identity = review.source().entries()[0].observation();
        let identity = match identity {
            crate::file_transfer::SourceObservation::Ready(value) => value.identity(),
            _ => panic!("source"),
        };
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        jobs.enqueue(plan).unwrap();
        jobs.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let info = loop {
            let snapshot = jobs.snapshot();
            if let Some(info) = snapshot.history.into_iter().next() {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Completed);
        let history = ReceiptHistory::new(&root);
        let receipt = info.receipt_id.unwrap();
        let undo = history.prepare_undo(receipt, 0, &cancel).unwrap();
        fs::write(&source, b"foreign").unwrap();
        assert!(undo.approve(&cancel).is_err());
        fs::remove_file(&source).unwrap();
        let undo = history.prepare_undo(receipt, 0, &cancel).unwrap();
        let interrupted_plan = undo.approve(&cancel).unwrap();
        let mut store = ReceiptStore::new(&root);
        let mut interrupted = store.admit(&interrupted_plan).unwrap();
        let interrupted_id = TransferReceiptId(interrupted.receipt_id());
        let crate::file_transfer::ApprovedTarget::CreateAt(target) =
            interrupted_plan.entries()[0].destination()
        else {
            panic!("undo target")
        };
        interrupted.begin_operation().unwrap();
        interrupted
            .begin_entry(interrupted_plan.entries()[0].id(), target)
            .unwrap();
        interrupted.end_operation();
        drop(interrupted);
        assert!(matches!(
            history.prepare_retry(interrupted_id, 0, &cancel),
            Err(RecoveryActionError::NotEligible)
        ));
        let late_plan = history
            .prepare_undo(receipt, 0, &cancel)
            .unwrap()
            .approve(&cancel)
            .unwrap();
        fs::write(&source, b"late foreign").unwrap();
        let mut late_receipt = store.admit(&late_plan).unwrap();
        let late = crate::file_transfer::execute::execute_with_journal(
            &late_plan,
            &CopyControl::default(),
            |_| {},
            &mut late_receipt,
        )
        .unwrap();
        assert!(matches!(
            late.rows[0].state,
            crate::file_transfer::ExecutionState::Failed(_)
        ));
        assert_eq!(fs::read(&source).unwrap(), b"late foreign");
        assert_eq!(fs::read(out.join("original.txt")).unwrap(), b"content");
        assert_eq!(
            fs::read(zone(&out.join("original.txt"))).unwrap(),
            zone_data
        );
        drop(late_receipt);
        fs::remove_file(&source).unwrap();
        let plan = history
            .prepare_undo(receipt, 0, &cancel)
            .unwrap()
            .approve(&cancel)
            .unwrap();
        let mut receipt = store.admit(&plan).unwrap();
        let report = crate::file_transfer::execute::execute_with_journal(
            &plan,
            &CopyControl::default(),
            |_| {},
            &mut receipt,
        )
        .unwrap();
        assert!(matches!(
            report.rows[0].state,
            crate::file_transfer::ExecutionState::Moved(_)
        ));
        assert_eq!(
            crate::file_transfer::inspect_source(&source, &cancel)
                .unwrap()
                .identity(),
            identity
        );
        assert_eq!(fs::read(&source).unwrap(), b"content");
        assert_eq!(fs::read(zone(&source)).unwrap(), zone_data);
        assert!(!out.join("original.txt").exists());
    }
    #[test]
    fn receipt_actions_undo_rejects_changed_zone_without_moving() {
        use crate::activity::ActivityCenter;
        use crate::file_transfer::{TransferJobStatus, TransferJobs};
        use std::time::{Duration, Instant};
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("original.txt");
        let out = temp.path().join("out");
        fs::write(&source, b"content").unwrap();
        fs::write(zone(&source), b"zone-original").unwrap();
        fs::create_dir(&out).unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::MoveTo(out.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        jobs.enqueue(plan).unwrap();
        jobs.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let info = loop {
            let state = jobs.snapshot();
            if let Some(info) = state.history.into_iter().next() {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Completed);
        let history = ReceiptHistory::new(&root);
        let undo = history
            .prepare_undo(info.receipt_id.unwrap(), 0, &cancel)
            .unwrap();
        let moved = out.join("original.txt");
        fs::write(zone(&moved), b"zone-modified").unwrap();
        if let Ok(plan) = undo.approve(&cancel) {
            let mut store = ReceiptStore::new(&root);
            let mut receipt = store.admit(&plan).unwrap();
            let report = crate::file_transfer::execute::execute_with_journal(
                &plan,
                &CopyControl::default(),
                |_| {},
                &mut receipt,
            )
            .unwrap();
            assert!(matches!(
                report.rows[0].state,
                crate::file_transfer::ExecutionState::Failed(_)
            ));
        }
        assert!(!source.exists());
        assert_eq!(fs::read(&moved).unwrap(), b"content");
        assert_eq!(fs::read(zone(&moved)).unwrap(), b"zone-modified");
    }
}
