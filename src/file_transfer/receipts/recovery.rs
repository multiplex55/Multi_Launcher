//! Read-only receipt history. Disk evidence never grants transfer authority.
use super::store::{CatalogEntry, ReceiptDiagnostic, ReceiptStore};
use super::wire::{
    self, Effect, EntryOutcome, Kind, NodeId, Object, Place, PlannedTarget, Record, Revision,
    SemanticPrefix, Verification,
};
use crate::file_operations::{NativeFileIdentity, ScanCancellation};
use crate::file_transfer::{
    EntryMetadata, HeldParent, TargetOccupancy, TransferLeaf, TransferReceiptId,
};
use crate::platform::app_data::AppDataRoot;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptHealth {
    Interrupted,
    Completed,
    Partial,
    Cancelled,
    Failed,
    Corrupt,
    Unsupported,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptEntryStatus {
    Remaining,
    Skipped,
    Completed,
    CompletedFromEvidence,
    DestinationCreated,
    RetainedByReview,
    Failed,
    Cancelled,
    Blocked,
    Ambiguous,
}
#[derive(Clone, Debug)]
pub struct ReceiptSummary {
    pub id: Option<TransferReceiptId>,
    pub health: ReceiptHealth,
    pub bytes: Option<u64>,
    /// Windows FILETIME creation timestamp from the receipt file.
    pub created_filetime: Option<u64>,
    pub entries: usize,
    pub completed: usize,
    pub remaining: usize,
    pub settled: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptEvidenceRole {
    Source,
    Output,
    MergeTarget,
    Stage,
    Backup,
    Quarantine,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiptObservation {
    Unassigned,
    Vacant,
    SameRevision,
    SameIdentity,
    Changed,
    Foreign(NativeFileIdentity),
    NameChanged,
    Unavailable,
}
#[derive(Clone, Debug)]
pub struct ReceiptEvidence {
    pub role: ReceiptEvidenceRole,
    pub path: PathBuf,
    pub expected_identity: Option<NativeFileIdentity>,
    pub recorded_revision: Option<EntryMetadata>,
    pub observation: ReceiptObservation,
}
#[derive(Clone, Debug)]
pub struct VerifiedMoveEvidence {
    pub source: PathBuf,
    pub output: PathBuf,
    pub output_identity: NativeFileIdentity,
    pub output_revision: EntryMetadata,
    pub sha256: [u8; 32],
    pub zone_sha256: Option<[u8; 32]>,
    pub(in crate::file_transfer) source_parent: crate::file_transfer::ParentSnapshot,
    pub(in crate::file_transfer) output_parent: crate::file_transfer::ParentSnapshot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptTargetChoice {
    Vacant,
    Replace,
    Merge,
}
#[derive(Clone, Debug)]
pub enum ReceiptTarget {
    Direct {
        path: PathBuf,
        parent_identity: NativeFileIdentity,
        choice: ReceiptTargetChoice,
        incumbent_identity: Option<NativeFileIdentity>,
        incumbent_revision: Option<EntryMetadata>,
    },
    Child {
        parent_node: u32,
        leaf: std::ffi::OsString,
    },
    Skipped,
}
#[derive(Clone, Debug)]
pub struct ReceiptEntryDetail {
    pub index: u32,
    pub parent: Option<u32>,
    pub status: ReceiptEntryStatus,
    pub intended_target: ReceiptTarget,
    pub evidence: Vec<ReceiptEvidence>,
    pub verified_atomic_move: Option<VerifiedMoveEvidence>,
}
#[derive(Clone, Debug)]
pub struct ReceiptDetail {
    pub summary: ReceiptSummary,
    pub entries: Vec<ReceiptEntryDetail>,
    pub trusted_prefix: bool,
    pub outline_complete: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptHistoryError {
    Unavailable,
    NotFound,
    Duplicate,
    Cancelled,
    InvalidEvidence,
}
impl std::fmt::Display for ReceiptHistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Receipt history is unavailable.",
            Self::NotFound => "Receipt was not found.",
            Self::Duplicate => "Receipt identity is duplicated.",
            Self::Cancelled => "Receipt reconciliation was cancelled.",
            Self::InvalidEvidence => "Receipt evidence is invalid.",
        })
    }
}
impl std::error::Error for ReceiptHistoryError {}

enum Store<'a> {
    Owned(ReceiptStore),
    Borrowed(&'a ReceiptStore),
}
pub struct ReceiptHistory<'a> {
    store: Store<'a>,
}
impl ReceiptHistory<'static> {
    pub fn new(root: &AppDataRoot) -> Self {
        Self {
            store: Store::Owned(ReceiptStore::new(root)),
        }
    }
}
impl<'a> ReceiptHistory<'a> {
    pub(in crate::file_transfer) fn from_store(store: &'a ReceiptStore) -> Self {
        Self {
            store: Store::Borrowed(store),
        }
    }
    fn store(&self) -> &ReceiptStore {
        match &self.store {
            Store::Owned(store) => store,
            Store::Borrowed(store) => store,
        }
    }
    pub fn summaries(&self) -> Result<Vec<ReceiptSummary>, ReceiptHistoryError> {
        let catalog = self
            .store()
            .scan()
            .map_err(|_| ReceiptHistoryError::Unavailable)?;
        let mut summaries: Vec<_> = catalog.entries.iter().map(summary).collect();
        summaries.extend(catalog.unavailable.iter().map(|name| ReceiptSummary {
            id: super::store::parse_name(name).map(|id| TransferReceiptId(id.0)),
            health: ReceiptHealth::Unavailable,
            bytes: None,
            created_filetime: None,
            entries: 0,
            completed: 0,
            remaining: 0,
            settled: false,
        }));
        Ok(summaries)
    }
    pub fn reconcile(
        &self,
        id: TransferReceiptId,
        cancel: &ScanCancellation,
    ) -> Result<ReceiptDetail, ReceiptHistoryError> {
        if cancel.is_cancelled() {
            return Err(ReceiptHistoryError::Cancelled);
        }
        let entry = self
            .store()
            .read_selected(wire::ReceiptId(id.0))
            .map_err(|_| ReceiptHistoryError::Unavailable)?
            .ok_or(ReceiptHistoryError::NotFound)?;
        reconcile_entry(&entry, cancel)
    }
}

fn summary(entry: &CatalogEntry) -> ReceiptSummary {
    let prefix = &entry.prefix;
    let mut count = 0;
    let mut completed = 0;
    let mut terminal = 0;
    let mut finished = None;
    for record in &prefix.records {
        match record {
            Record::Node(_) => count += 1,
            Record::EntryTerminal { outcome, .. } => {
                terminal += 1;
                if matches!(outcome, EntryOutcome::Completed) {
                    completed += 1;
                }
            }
            Record::Finished { outcome } => finished = Some(*outcome),
            _ => {}
        }
    }
    let health = match &entry.diagnostic {
        ReceiptDiagnostic::Clean => match finished {
            None => ReceiptHealth::Interrupted,
            Some(wire::JobOutcome::Completed) => ReceiptHealth::Completed,
            Some(wire::JobOutcome::Partial) => ReceiptHealth::Partial,
            Some(wire::JobOutcome::Cancelled) => ReceiptHealth::Cancelled,
            Some(wire::JobOutcome::Failed) => ReceiptHealth::Failed,
        },
        ReceiptDiagnostic::Oversized | ReceiptDiagnostic::DecodeBudget => {
            ReceiptHealth::Unsupported
        }
        _ => ReceiptHealth::Corrupt,
    };
    ReceiptSummary {
        id: super::store::parse_name(&entry.leaf).map(|id| TransferReceiptId(id.0)),
        health,
        bytes: Some(entry.bytes),
        created_filetime: Some(entry.created),
        entries: count,
        completed,
        remaining: count.saturating_sub(terminal),
        settled: entry.settled,
    }
}

#[derive(Clone)]
struct Candidate {
    role: ReceiptEvidenceRole,
    place: Place,
    identity: Option<NativeFileIdentity>,
    revision: Option<Revision>,
}
struct Node {
    parent: Option<u32>,
    status: ReceiptEntryStatus,
    intended_target: ReceiptTarget,
    verified_output: bool,
    atomic_vacant: bool,
    atomic_observed: bool,
    staged_removed: bool,
    directory_created: bool,
    directory_metadata: bool,
    directory_removed: bool,
    incumbent_revision: Option<Revision>,
    candidates: Vec<Candidate>,
    atomic: Option<(Object, Revision, [u8; 32], Option<[u8; 32]>)>,
}
fn put(node: &mut Node, candidate: Candidate) {
    if let Some(existing) = node
        .candidates
        .iter_mut()
        .find(|item| item.role == candidate.role && item.place == candidate.place)
    {
        if candidate.identity.is_some() {
            existing.identity = candidate.identity;
        }
        if candidate.revision.is_some() {
            existing.revision = candidate.revision;
        }
    } else {
        node.candidates.push(candidate);
    }
}
fn item(role: ReceiptEvidenceRole, obj: &Object, revision: Option<Revision>) -> Candidate {
    Candidate {
        role,
        place: obj.place.clone(),
        identity: Some(obj.identity),
        revision,
    }
}
fn unknown(role: ReceiptEvidenceRole, place: &Place) -> Candidate {
    Candidate {
        role,
        place: place.clone(),
        identity: None,
        revision: None,
    }
}
fn target(
    prefix: &SemanticPrefix,
    target: &PlannedTarget,
) -> Result<ReceiptTarget, ReceiptHistoryError> {
    use std::os::windows::ffi::OsStringExt;
    match target {
        PlannedTarget::Skipped => Ok(ReceiptTarget::Skipped),
        PlannedTarget::Child { parent, leaf } => Ok(ReceiptTarget::Child {
            parent_node: parent.0,
            leaf: std::ffi::OsString::from_wide(&leaf.0),
        }),
        PlannedTarget::Direct { place, policy } => {
            let parent = prefix
                .parent_snapshot(place.parent)
                .map_err(|_| ReceiptHistoryError::InvalidEvidence)?;
            let (choice, incumbent_identity, incumbent_revision) = match policy {
                wire::TargetPolicy::Vacant => (ReceiptTargetChoice::Vacant, None, None),
                wire::TargetPolicy::Replace {
                    incumbent,
                    revision,
                } => (
                    ReceiptTargetChoice::Replace,
                    Some(incumbent.identity),
                    Some((*revision).into()),
                ),
                wire::TargetPolicy::Merge {
                    incumbent,
                    revision,
                } => (
                    ReceiptTargetChoice::Merge,
                    Some(incumbent.identity),
                    Some((*revision).into()),
                ),
            };
            Ok(ReceiptTarget::Direct {
                path: parent
                    .path()
                    .join(std::ffi::OsString::from_wide(&place.leaf.0)),
                parent_identity: parent.identity(),
                choice,
                incumbent_identity,
                incumbent_revision,
            })
        }
    }
}
fn reconcile_entry(
    entry: &CatalogEntry,
    cancel: &ScanCancellation,
) -> Result<ReceiptDetail, ReceiptHistoryError> {
    let summary = summary(entry);
    let trusted_prefix = matches!(entry.diagnostic, ReceiptDiagnostic::Clean);
    let mut nodes = BTreeMap::<u32, Node>::new();
    let mut action = None;
    for record in &entry.prefix.records {
        if cancel.is_cancelled() {
            return Err(ReceiptHistoryError::Cancelled);
        }
        match record {
            Record::Header { action: value, .. } => action = Some(*value),
            Record::Node(outline) => {
                let intended_target = target(&entry.prefix, &outline.target)?;
                let mut node = Node {
                    parent: outline.parent.map(|p| p.0),
                    intended_target,
                    verified_output: false,
                    atomic_vacant: false,
                    atomic_observed: false,
                    staged_removed: false,
                    directory_created: false,
                    directory_metadata: false,
                    directory_removed: false,
                    incumbent_revision: match &outline.target {
                        PlannedTarget::Direct {
                            policy:
                                wire::TargetPolicy::Replace { revision, .. }
                                | wire::TargetPolicy::Merge { revision, .. },
                            ..
                        } => Some(*revision),
                        _ => None,
                    },
                    status: if matches!(outline.target, PlannedTarget::Skipped) {
                        ReceiptEntryStatus::Skipped
                    } else {
                        ReceiptEntryStatus::Remaining
                    },
                    candidates: Vec::new(),
                    atomic: None,
                };
                if let Some(source) = &outline.source {
                    put(
                        &mut node,
                        item(ReceiptEvidenceRole::Source, source, outline.revision),
                    );
                }
                if let PlannedTarget::Direct {
                    policy:
                        wire::TargetPolicy::Merge {
                            incumbent,
                            revision,
                        },
                    ..
                } = &outline.target
                {
                    put(
                        &mut node,
                        item(ReceiptEvidenceRole::MergeTarget, incumbent, Some(*revision)),
                    );
                }
                nodes.insert(outline.id.0, node);
            }
            Record::EntryTerminal { node, outcome, .. } => {
                if let Some(row) = nodes.get_mut(&node.0) {
                    row.status = match outcome {
                        EntryOutcome::Completed => ReceiptEntryStatus::Completed,
                        EntryOutcome::Skipped => ReceiptEntryStatus::Skipped,
                        EntryOutcome::RetainedByReview => ReceiptEntryStatus::RetainedByReview,
                        EntryOutcome::Failed => ReceiptEntryStatus::Failed,
                        EntryOutcome::Cancelled => ReceiptEntryStatus::Cancelled,
                        EntryOutcome::Blocked => ReceiptEntryStatus::Blocked,
                    };
                }
            }
            Record::Effect { node, effect } => {
                if let Some(row) = nodes.get_mut(&node.0) {
                    match effect {
                        Effect::StageIntent { stage } => {
                            put(row, unknown(ReceiptEvidenceRole::Stage, stage))
                        }
                        Effect::StageCreated { stage } => {
                            put(row, item(ReceiptEvidenceRole::Stage, stage, None))
                        }
                        Effect::DisplacementIntent { original, backup }
                        | Effect::DisplacementObserved { original, backup }
                        | Effect::RestoreIntent { original, backup }
                        | Effect::RestoreObserved { original, backup } => {
                            let revision = row.incumbent_revision;
                            put(
                                row,
                                Candidate {
                                    role: ReceiptEvidenceRole::Backup,
                                    place: backup.clone(),
                                    identity: Some(original.identity),
                                    revision,
                                },
                            );
                        }
                        Effect::PublishIntent { stage, target } => put(
                            row,
                            Candidate {
                                role: ReceiptEvidenceRole::Output,
                                place: target.clone(),
                                identity: Some(stage.identity),
                                revision: None,
                            },
                        ),
                        Effect::PublicationObserved { output } => {
                            put(row, item(ReceiptEvidenceRole::Output, output, None))
                        }
                        Effect::DirectoryCreated { output, .. } => {
                            row.directory_created = true;
                            put(row, item(ReceiptEvidenceRole::Output, output, None));
                        }
                        Effect::Verified {
                            output, revision, ..
                        } => {
                            row.verified_output = true;
                            put(
                                row,
                                item(ReceiptEvidenceRole::Output, output, Some(*revision)),
                            );
                        }
                        Effect::DirectoryMetadataObserved { output, revision } => {
                            row.directory_metadata = true;
                            put(
                                row,
                                item(ReceiptEvidenceRole::Output, output, Some(*revision)),
                            );
                        }
                        Effect::MoveMetadataObserved { output, revision } => put(
                            row,
                            item(ReceiptEvidenceRole::Output, output, Some(*revision)),
                        ),
                        Effect::MoveRenameIntent { source, target, .. } => put(
                            row,
                            Candidate {
                                role: ReceiptEvidenceRole::Output,
                                place: target.clone(),
                                identity: Some(source.identity),
                                revision: None,
                            },
                        ),
                        Effect::MoveRenameObserved {
                            at_target,
                            source_disposition,
                            ..
                        } => {
                            row.atomic_observed = *at_target == Some(true);
                            row.atomic_vacant = row.atomic_observed
                                && *source_disposition == wire::AtomicSourceDisposition::Vacant;
                        }
                        Effect::MoveVerified {
                            output,
                            revision,
                            verification,
                        } => {
                            row.verified_output = true;
                            put(
                                row,
                                item(ReceiptEvidenceRole::Output, output, Some(*revision)),
                            );
                            if let Verification::Verified {
                                sha256,
                                zone_sha256,
                            } = verification
                            {
                                row.atomic =
                                    Some((output.clone(), *revision, *sha256, *zone_sha256));
                            }
                        }
                        Effect::StagedSourceObserved {
                            action: wire::SourceAction::Dispose,
                            state: wire::SourceState::Removed,
                            source,
                            recovery: Some(place),
                            ..
                        } => {
                            row.staged_removed = true;
                            put(
                                row,
                                Candidate {
                                    role: ReceiptEvidenceRole::Quarantine,
                                    place: place.clone(),
                                    identity: Some(source.identity),
                                    revision: None,
                                },
                            );
                        }
                        Effect::StagedSourceIntent {
                            source,
                            recovery: Some(place),
                            ..
                        }
                        | Effect::StagedSourceObserved {
                            source,
                            recovery: Some(place),
                            ..
                        } => put(
                            row,
                            Candidate {
                                role: ReceiptEvidenceRole::Quarantine,
                                place: place.clone(),
                                identity: Some(source.identity),
                                revision: None,
                            },
                        ),
                        Effect::DirectoryRemoveObserved {
                            observation: wire::Removal::Removed | wire::Removal::Replaced(_),
                            ..
                        } => {
                            row.directory_removed = true;
                        }
                        Effect::DirectoryCreateIntent { target } => {
                            put(row, unknown(ReceiptEvidenceRole::Output, target))
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let mut incomplete_children = vec![false; nodes.len()];
    for (&index, node) in nodes.iter_mut().rev() {
        if entry.prefix.pending(NodeId(index)) {
            node.status = ReceiptEntryStatus::Ambiguous;
        } else if node.status == ReceiptEntryStatus::Remaining {
            if node.directory_removed
                || (node.verified_output
                    && (matches!(action, Some(wire::Action::Copy | wire::Action::Duplicate))
                        || node.atomic_observed
                        || node.staged_removed))
                || (node.directory_metadata
                    && matches!(action, Some(wire::Action::Copy | wire::Action::Duplicate))
                    && !incomplete_children[index as usize])
            {
                node.status = ReceiptEntryStatus::CompletedFromEvidence;
            } else if node.directory_created {
                node.status = ReceiptEntryStatus::DestinationCreated;
            }
        }
        if let Some(parent) = node.parent {
            if !matches!(
                node.status,
                ReceiptEntryStatus::Completed
                    | ReceiptEntryStatus::CompletedFromEvidence
                    | ReceiptEntryStatus::Skipped
                    | ReceiptEntryStatus::RetainedByReview
            ) {
                incomplete_children[parent as usize] = true;
            }
        }
    }
    let mut details = Vec::with_capacity(nodes.len());
    for (index, node) in nodes {
        let mut evidence = Vec::with_capacity(node.candidates.len());
        for candidate in &node.candidates {
            evidence.push(probe(&entry.prefix, candidate, cancel)?);
        }
        let atomic = if trusted_prefix
            && summary.health == ReceiptHealth::Completed
            && node.status == ReceiptEntryStatus::Completed
            && node.atomic_vacant
        {
            node.atomic
                .as_ref()
                .and_then(|(output, revision, sha256, zone_sha256)| {
                    let source = evidence
                        .iter()
                        .find(|e| e.role == ReceiptEvidenceRole::Source)?;
                    let target = evidence.iter().find(|e| {
                        e.role == ReceiptEvidenceRole::Output
                            && e.expected_identity == Some(output.identity)
                    })?;
                    if source.observation != ReceiptObservation::Vacant
                        || target.observation != ReceiptObservation::SameRevision
                    {
                        return None;
                    }
                    Some(VerifiedMoveEvidence {
                        source: source.path.clone(),
                        output: target.path.clone(),
                        output_identity: output.identity,
                        output_revision: (*revision).into(),
                        sha256: *sha256,
                        zone_sha256: *zone_sha256,
                        source_parent: entry
                            .prefix
                            .parent_snapshot(
                                node.candidates
                                    .iter()
                                    .find(|c| c.role == ReceiptEvidenceRole::Source)?
                                    .place
                                    .parent,
                            )
                            .ok()?,
                        output_parent: entry.prefix.parent_snapshot(output.place.parent).ok()?,
                    })
                })
        } else {
            None
        };
        details.push(ReceiptEntryDetail {
            index,
            parent: node.parent,
            status: node.status,
            intended_target: node.intended_target,
            evidence,
            verified_atomic_move: atomic,
        });
    }
    let outline_complete = entry.prefix.records.iter().any(|r| {
        matches!(
            r,
            Record::Bind(_)
                | Record::Effect { .. }
                | Record::EntryTerminal { .. }
                | Record::Finished { .. }
        )
    });
    Ok(ReceiptDetail {
        summary,
        entries: details,
        trusted_prefix,
        outline_complete,
    })
}
impl From<Revision> for EntryMetadata {
    fn from(r: Revision) -> Self {
        Self {
            kind: match r.kind {
                Kind::File => crate::file_transfer::EntryKind::File,
                Kind::Directory => crate::file_transfer::EntryKind::Directory,
            },
            byte_len: r.length,
            attributes: r.attributes,
            created: r.created,
            modified: r.modified,
            changed: r.changed,
        }
    }
}
fn probe(
    prefix: &SemanticPrefix,
    candidate: &Candidate,
    cancel: &ScanCancellation,
) -> Result<ReceiptEvidence, ReceiptHistoryError> {
    use std::os::windows::ffi::OsStringExt;
    if cancel.is_cancelled() {
        return Err(ReceiptHistoryError::Cancelled);
    }
    let parent = prefix
        .parent_snapshot(candidate.place.parent)
        .map_err(|_| ReceiptHistoryError::InvalidEvidence)?;
    let name = std::ffi::OsString::from_wide(&candidate.place.leaf.0);
    let path = parent.path().join(&name);
    let mut result = ReceiptEvidence {
        role: candidate.role,
        path,
        expected_identity: candidate.identity,
        recorded_revision: candidate.revision.map(Into::into),
        observation: ReceiptObservation::Unassigned,
    };
    let Some(expected) = candidate.identity else {
        return Ok(result);
    };
    let observed = (|| {
        let held = HeldParent::open(parent.path(), cancel).ok()?;
        let (actual_root, actual_parts) =
            crate::archive::transfer_native::split_local_absolute_path(held.snapshot.path())
                .ok()?;
        let (expected_root, expected_parts) =
            crate::archive::transfer_native::split_local_absolute_path(parent.path()).ok()?;
        if held.snapshot.chain != parent.chain
            || actual_root != expected_root
            || actual_parts != expected_parts
        {
            return None;
        }
        let leaf = TransferLeaf::new(&name).ok()?;
        match held.inspect_leaf(&leaf, cancel).ok()? {
            TargetOccupancy::Vacant => Some(ReceiptObservation::Vacant),
            TargetOccupancy::Occupied(entry) => {
                if entry.leaf().as_os_str() != name {
                    return Some(ReceiptObservation::NameChanged);
                }
                if entry.identity() != expected {
                    return Some(ReceiptObservation::Foreign(entry.identity()));
                }
                Some(match candidate.revision {
                    Some(revision) if Revision::from(entry.metadata()) == revision => {
                        ReceiptObservation::SameRevision
                    }
                    Some(_) => ReceiptObservation::Changed,
                    None => ReceiptObservation::SameIdentity,
                })
            }
        }
    })();
    if cancel.is_cancelled() {
        return Err(ReceiptHistoryError::Cancelled);
    }
    result.observation = observed.unwrap_or(ReceiptObservation::Unavailable);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_transfer::journal::{TransferJournal, TransferJournalEvent};
    use crate::file_transfer::{
        ApprovedTransferPlan, CopyControl, TransferAction, TransferDraft, inspect_source_tree,
        review_transfer,
    };
    use std::{fs, io::Write, sync::Arc};
    fn fixture(root: &std::path::Path) -> (ApprovedTransferPlan, PathBuf, PathBuf) {
        let source = root.join("source.txt");
        let output = root.join("out");
        fs::write(&source, b"original").unwrap();
        fs::create_dir(&output).unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(output.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        (
            review.approve(review.generation(), &cancel).unwrap(),
            source,
            output,
        )
    }
    #[test]
    fn receipt_recovery_interrupted_real_copy_and_foreign_output() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let (plan, source, output) = fixture(temp.path());
        let mut store = ReceiptStore::new(&root);
        let mut session = store.admit(&plan).unwrap();
        let id = TransferReceiptId(session.receipt_id());
        let report = crate::file_transfer::execute::execute_with_journal(
            &plan,
            &crate::file_transfer::CopyControl::default(),
            |_| {},
            &mut session,
        )
        .unwrap();
        assert!(matches!(
            report.rows[0].state,
            crate::file_transfer::ExecutionState::Copied(_)
        ));
        drop(session);
        let history = ReceiptHistory::from_store(&store);
        let summaries = history.summaries().unwrap();
        assert_eq!(summaries[0].health, ReceiptHealth::Interrupted);
        let before = fs::read(output.join("source.txt")).unwrap();
        let detail = history.reconcile(id, &ScanCancellation::new()).unwrap();
        assert!(detail.trusted_prefix);
        assert_eq!(
            detail.entries[0].status,
            ReceiptEntryStatus::CompletedFromEvidence
        );
        assert!(
            detail.entries[0]
                .evidence
                .iter()
                .any(|e| e.role == ReceiptEvidenceRole::Output
                    && e.observation == ReceiptObservation::SameRevision)
        );
        fs::remove_file(output.join("source.txt")).unwrap();
        fs::write(output.join("source.txt"), b"foreign").unwrap();
        let detail = history.reconcile(id, &ScanCancellation::new()).unwrap();
        assert!(
            detail.entries[0]
                .evidence
                .iter()
                .any(|e| e.role == ReceiptEvidenceRole::Output
                    && matches!(e.observation, ReceiptObservation::Foreign(_)))
        );
        assert_eq!(fs::read(source).unwrap(), before);
        assert_eq!(fs::read(output.join("source.txt")).unwrap(), b"foreign");
    }
    #[test]
    fn receipt_recovery_unobserved_stage_and_torn_prefix_never_claim_occupant() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let (plan, _, output) = fixture(temp.path());
        let mut store = ReceiptStore::new(&root);
        let mut session = store.admit(&plan).unwrap();
        let id = TransferReceiptId(session.receipt_id());
        let entry = &plan.entries()[0];
        let crate::file_transfer::ApprovedTarget::CreateAt(target) = entry.destination() else {
            panic!("vacant target")
        };
        session.begin_operation().unwrap();
        session.begin_entry(entry.id(), target).unwrap();
        let stage = output.join("protected-stage");
        TransferJournal::record(
            &mut session,
            TransferJournalEvent::StageIntent {
                path: stage.clone(),
                parent: target.parent().identity(),
            },
        )
        .unwrap();
        session.end_operation();
        drop(session);
        fs::write(&stage, b"foreign").unwrap();
        let path = temp
            .path()
            .join("transfers")
            .join("receipts")
            .join(format!("{id}.receipt"));
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[7])
            .unwrap();
        let bytes = fs::read(&path).unwrap();
        let detail = ReceiptHistory::from_store(&store)
            .reconcile(id, &ScanCancellation::new())
            .unwrap();
        assert_eq!(detail.summary.health, ReceiptHealth::Corrupt);
        assert!(!detail.trusted_prefix);
        assert_eq!(detail.entries[0].status, ReceiptEntryStatus::Ambiguous);
        assert!(
            detail.entries[0]
                .evidence
                .iter()
                .any(|e| e.role == ReceiptEvidenceRole::Stage
                    && e.expected_identity.is_none()
                    && e.observation == ReceiptObservation::Unassigned)
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert_eq!(fs::read(stage).unwrap(), b"foreign");
    }
    #[test]
    fn receipt_recovery_nested_parent_timestamp_proof_tracks_recorded_updates() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("source-dir");
        let output = temp.path().join("out");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir(&output).unwrap();
        fs::write(source.join("nested").join("file"), b"data").unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(output.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut store = ReceiptStore::new(&root);
        let mut session = store.admit(&plan).unwrap();
        let id = TransferReceiptId(session.receipt_id());
        let report = crate::file_transfer::execute::execute_with_journal(
            &plan,
            &CopyControl::default(),
            |_| {},
            &mut session,
        )
        .unwrap();
        assert!(matches!(
            report.rows.last().unwrap().state,
            crate::file_transfer::ExecutionState::Copied(_)
        ));
        drop(session);
        let detail = ReceiptHistory::from_store(&store)
            .reconcile(id, &ScanCancellation::new())
            .unwrap();
        let file = detail.entries.last().unwrap();
        assert!(
            file.evidence
                .iter()
                .any(|e| e.role == ReceiptEvidenceRole::Output
                    && e.observation == ReceiptObservation::SameRevision)
        );
        assert_eq!(
            fs::read(output.join("source-dir/nested/file")).unwrap(),
            b"data"
        );
    }
    #[test]
    fn receipt_recovery_completed_atomic_move_requires_vacant_source_and_unchanged_output() {
        use crate::activity::ActivityCenter;
        use crate::file_transfer::{TransferJobStatus, TransferJobs};
        use std::time::{Duration, Instant};
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("move.txt");
        let output = temp.path().join("out");
        fs::write(&source, b"data").unwrap();
        fs::create_dir(&output).unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::MoveTo(output.clone()),
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
            let snapshot = jobs.snapshot();
            if let Some(info) = snapshot.history.into_iter().next() {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Completed);
        let history = ReceiptHistory::new(&root);
        let id = info.receipt_id.unwrap();
        let detail = history.reconcile(id, &ScanCancellation::new()).unwrap();
        assert!(detail.entries[0].verified_atomic_move.is_some());
        fs::write(output.join("move.txt"), b"changed").unwrap();
        let detail = history.reconcile(id, &ScanCancellation::new()).unwrap();
        assert!(detail.entries[0].verified_atomic_move.is_none());
    }
    #[test]
    fn receipt_recovery_active_receipt_does_not_hide_old_history() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let (plan, _, _) = fixture(temp.path());
        let mut store = ReceiptStore::new(&root);
        let mut prior = store.admit(&plan).unwrap();
        let old_id = TransferReceiptId(prior.receipt_id());
        prior
            .finish_entry(
                plan.source().entries()[0].id(),
                wire::EntryOutcome::Cancelled,
                wire::Disposition::NoEffect,
            )
            .unwrap();
        prior.finish_job(wire::JobOutcome::Cancelled).unwrap();
        drop(prior);
        let live = store.admit(&plan).unwrap();
        let live_id = TransferReceiptId(live.receipt_id());
        let history = ReceiptHistory::new(&root);
        let summaries = history.summaries().unwrap();
        assert!(
            summaries
                .iter()
                .any(|row| row.id == Some(old_id) && row.health == ReceiptHealth::Cancelled)
        );
        assert!(summaries.iter().any(|row| row.id == Some(live_id)
            && row.health == ReceiptHealth::Unavailable
            && row.bytes.is_none()));
        assert_eq!(
            history
                .reconcile(old_id, &ScanCancellation::new())
                .unwrap()
                .summary
                .health,
            ReceiptHealth::Cancelled
        );
        assert!(matches!(
            history.reconcile(live_id, &ScanCancellation::new()),
            Err(ReceiptHistoryError::Unavailable)
        ));
    }
    #[test]
    fn receipt_recovery_case_renamed_and_unsafe_sibling_do_not_hide_valid_history() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let (plan, _, _) = fixture(temp.path());
        let mut store = ReceiptStore::new(&root);
        let make = |store: &mut ReceiptStore| {
            let mut receipt = store.admit(&plan).unwrap();
            let id = TransferReceiptId(receipt.receipt_id());
            receipt
                .finish_entry(
                    plan.source().entries()[0].id(),
                    wire::EntryOutcome::Cancelled,
                    wire::Disposition::NoEffect,
                )
                .unwrap();
            receipt.finish_job(wire::JobOutcome::Cancelled).unwrap();
            id
        };
        let valid_id = make(&mut store);
        let renamed_id = make(&mut store);
        let dir = temp.path().join("transfers").join("receipts");
        let canonical = dir.join(format!("{renamed_id}.receipt"));
        let intermediate = dir.join("temporary-name");
        fs::rename(&canonical, &intermediate).unwrap();
        fs::rename(&intermediate, dir.join(format!("{renamed_id}.RECEIPT"))).unwrap();
        let unsafe_id = TransferReceiptId([11; 16]);
        fs::create_dir(dir.join(format!("{unsafe_id}.receipt"))).unwrap();
        let catalog = store.scan().unwrap();
        assert!(catalog.admission_blocked);
        assert!(
            catalog
                .unavailable
                .iter()
                .any(|name| name == &std::ffi::OsString::from(format!("{unsafe_id}.receipt")))
        );
        let history = ReceiptHistory::from_store(&store);
        let summaries = history.summaries().unwrap();
        assert!(
            summaries
                .iter()
                .any(|s| s.id == Some(valid_id) && s.health == ReceiptHealth::Cancelled)
        );
        assert!(
            summaries
                .iter()
                .any(|s| s.id == Some(unsafe_id) && s.health == ReceiptHealth::Unavailable)
        );
        assert!(
            history
                .reconcile(valid_id, &ScanCancellation::new())
                .unwrap()
                .trusted_prefix
        );
        assert!(matches!(
            history.reconcile(renamed_id, &ScanCancellation::new()),
            Err(ReceiptHistoryError::Unavailable)
        ));
    }
}
