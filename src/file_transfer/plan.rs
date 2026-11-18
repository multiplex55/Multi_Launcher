//! Read-only destination review and exact conflict choices for transfer execution.
use super::{
    AdmissionError, EntryKind, EntrySnapshot, ParentSnapshot, SourceNodeId, SourceObservation,
    SourceTree, TargetOccupancy, TargetSnapshot, TransferLeaf, checked_parts, inspect_parent,
    inspect_target, recheck_source_metadata, recheck_target_metadata,
};
use crate::{
    batch_rename::compare_paths_windows_ordinal,
    file_operations::{NativeFileIdentity, ScanCancellation},
};
use std::os::windows::ffi::OsStrExt;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, HashSet},
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering},
    },
};

static NEXT_REVIEW: AtomicU64 = AtomicU64::new(1);
const MAX_DUPLICATE_CANDIDATES: usize = 1000;
const MAX_DUPLICATE_PROBES: usize = 10_000;
const MAX_REVIEW_BYTES: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferAction {
    CopyTo(PathBuf),
    MoveTo(PathBuf),
    Duplicate,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferDraft {
    pub action: TransferAction,
    /// Request a destination content check for a copy or duplicate. A move
    /// that publishes a new file always requires verification before removal.
    pub verify: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReviewGeneration(u64);
#[derive(Debug)]
pub enum ReviewError {
    Cancelled,
    Admission(AdmissionError),
    GenerationExhausted,
    Limit,
    StaleReview,
    InvalidChoice,
    ChoiceChanged(SourceNodeId),
    Unresolved(SourceNodeId),
    NothingToTransfer,
}
impl std::fmt::Display for ReviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Transfer review cancelled."),
            Self::Admission(error) => write!(f, "Transfer admission failed: {error}"),
            Self::StaleReview => {
                f.write_str("The transfer review changed. Select the action again.")
            }
            Self::InvalidChoice => {
                f.write_str("This choice is unavailable for the reviewed entries.")
            }
            Self::ChoiceChanged(node) => write!(
                f,
                "The reviewed destination for entry {} changed.",
                node.index() + 1
            ),
            Self::Unresolved(node) => write!(
                f,
                "Resolve entry {} before approving the transfer.",
                node.index() + 1
            ),
            Self::NothingToTransfer => f.write_str("The review has no approved transfer entries."),
            Self::Limit => f.write_str("Transfer destination review exceeds its limits."),
            Self::GenerationExhausted => f.write_str("Transfer review identity space exhausted."),
        }
    }
}
impl std::error::Error for ReviewError {}
#[derive(Debug)]
pub enum ReviewBlock {
    SourceUnavailable,
    DestinationInsideSource,
    TargetAliasesSource,
    NameCollision { other: SourceNodeId },
    Admission(AdmissionError),
    DuplicateNameExhausted,
}
#[derive(Debug)]
pub enum ReviewState {
    /// Target was inspected under a current, existing native parent.
    Vacant(TargetSnapshot),
    /// An occupied target requires a later explicit decision.
    Conflict(TargetSnapshot),
    /// This leaf is below a new, still-uncreated directory from this review.
    PlannedParent {
        directory: SourceNodeId,
        leaf: TransferLeaf,
    },
    /// Parent conflict or failure must be resolved before this row can map.
    WaitingParent {
        parent: SourceNodeId,
    },
    Blocked(ReviewBlock),
    Skipped,
    Replace(TargetSnapshot),
    Merge {
        target: TargetSnapshot,
        directory: ParentSnapshot,
    },
}

#[derive(Clone, Debug)]
pub enum ReviewChoice {
    Reset,
    Skip,
    KeepBoth(TransferLeaf),
    Replace,
    Merge,
}

#[derive(Clone, Copy, Debug)]
pub enum GroupChoice {
    Skip,
    Replace,
}

#[derive(Clone, Debug)]
enum BoundChoice {
    Skip,
    KeepBoth {
        parent: DestinationParent,
        leaf: TransferLeaf,
        vacancy: Option<TargetSnapshot>,
    },
    Replace(TargetSnapshot),
    Merge {
        target: TargetSnapshot,
        directory: ParentSnapshot,
    },
}

#[derive(Debug)]
pub struct ReviewRow {
    source: SourceNodeId,
    path: Option<PathBuf>,
    state: ReviewState,
}
impl ReviewRow {
    pub fn source(&self) -> SourceNodeId {
        self.source
    }
    pub fn destination_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn state(&self) -> &ReviewState {
        &self.state
    }
}
#[derive(Debug)]
pub struct TransferReview {
    generation: ReviewGeneration,
    queued: Arc<AtomicBool>,
    draft: TransferDraft,
    source: Arc<SourceTree>,
    rows: Vec<ReviewRow>,
    destination: Option<ParentSnapshot>,
    choices: HashMap<SourceNodeId, BoundChoice>,
}
impl TransferReview {
    pub fn generation(&self) -> ReviewGeneration {
        self.generation
    }
    pub fn draft(&self) -> &TransferDraft {
        &self.draft
    }
    pub fn source(&self) -> &Arc<SourceTree> {
        &self.source
    }
    pub fn rows(&self) -> &[ReviewRow] {
        &self.rows
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Namespace {
    Existing(NativeFileIdentity),
    Planned(SourceNodeId),
}
#[derive(Clone, Debug)]
struct OrdinalName(OsString);
impl PartialEq for OrdinalName {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for OrdinalName {}
impl PartialOrd for OrdinalName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrdinalName {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_paths_windows_ordinal(Path::new(&self.0), Path::new(&other.0))
    }
}
type Reservations = HashMap<Namespace, BTreeMap<OrdinalName, usize>>;
#[derive(Clone, Debug, PartialEq, Eq)]
enum DestinationParent {
    Existing(ParentSnapshot),
    Planned {
        directory: SourceNodeId,
        path: PathBuf,
    },
}
impl DestinationParent {
    fn path(&self) -> &Path {
        match self {
            Self::Existing(parent) => parent.path(),
            Self::Planned { path, .. } => path,
        }
    }
    fn namespace(&self) -> Namespace {
        match self {
            Self::Existing(parent) => Namespace::Existing(parent.identity()),
            Self::Planned { directory, .. } => Namespace::Planned(*directory),
        }
    }
}
fn cancelled(cancel: &ScanCancellation) -> Result<(), ReviewError> {
    if cancel.is_cancelled() {
        Err(ReviewError::Cancelled)
    } else {
        Ok(())
    }
}
fn duplicate_leaf(source: &TransferLeaf, number: usize) -> Result<TransferLeaf, AdmissionError> {
    let path = Path::new(source.as_os_str());
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or(AdmissionError::InvalidLeaf)?;
    let extension = path.extension().and_then(|s| s.to_str());
    let suffix = if number == 1 {
        " (copy)".to_owned()
    } else {
        format!(" (copy {number})")
    };
    let mut name = format!("{stem}{suffix}");
    if let Some(ext) = extension {
        name.push('.');
        name.push_str(ext);
    }
    TransferLeaf::new(name.as_ref())
}
fn duplicate_target(
    parent: &ParentSnapshot,
    leaf: &TransferLeaf,
    reservations: &Reservations,
    cancel: &ScanCancellation,
    probes: &mut usize,
) -> Result<Option<TargetSnapshot>, AdmissionError> {
    let namespace = Namespace::Existing(parent.identity());
    for number in 1..=MAX_DUPLICATE_CANDIDATES {
        if cancel.is_cancelled() {
            return Err(AdmissionError::Cancelled);
        }
        let candidate = duplicate_leaf(leaf, number)?;
        if reservations
            .get(&namespace)
            .is_some_and(|names| names.contains_key(&OrdinalName(candidate.as_os_str().to_owned())))
        {
            continue;
        }
        *probes = probes
            .checked_add(1)
            .filter(|count| *count <= MAX_DUPLICATE_PROBES)
            .ok_or(AdmissionError::Limit)?;
        let target = inspect_target(parent, &candidate, cancel)?;
        if matches!(target.occupancy(), TargetOccupancy::Vacant) {
            return Ok(Some(target));
        }
    }
    Ok(None)
}
/// Inspect only. A later decision layer must revalidate every snapshot before
/// authorizing any operation; source order and unavailable rows are retained.
pub fn review_transfer(
    source: Arc<SourceTree>,
    draft: TransferDraft,
    cancel: &ScanCancellation,
) -> Result<TransferReview, ReviewError> {
    cancelled(cancel)?;
    let destination = match &draft.action {
        TransferAction::CopyTo(path) | TransferAction::MoveTo(path) => {
            Some(inspect_parent(path, cancel).map_err(|error| {
                if matches!(error, AdmissionError::Cancelled) {
                    ReviewError::Cancelled
                } else {
                    ReviewError::Admission(error)
                }
            })?)
        }
        TransferAction::Duplicate => None,
    };
    build_review(source, draft, destination, HashMap::new(), cancel)
}

pub(super) fn review_exact_file(
    source: EntrySnapshot,
    target: TargetSnapshot,
    action: TransferAction,
    verify: bool,
    cancel: &ScanCancellation,
) -> Result<TransferReview, ReviewError> {
    if source.metadata().kind != EntryKind::File
        || !matches!(target.occupancy(), TargetOccupancy::Vacant)
    {
        return Err(ReviewError::InvalidChoice);
    }
    recheck_source_metadata(&source, cancel)?;
    recheck_target_metadata(&target, cancel)?;
    let tree = Arc::new(super::tree::singleton_file(source)?);
    let id = tree.entries()[0].id();
    let parent = target.parent().clone();
    let leaf = target.requested_leaf().clone();
    let mut choices = HashMap::new();
    choices.insert(
        id,
        BoundChoice::KeepBoth {
            parent: DestinationParent::Existing(parent.clone()),
            leaf,
            vacancy: Some(target),
        },
    );
    build_review(
        tree,
        TransferDraft { action, verify },
        Some(parent),
        choices,
        cancel,
    )
}

fn build_review(
    source: Arc<SourceTree>,
    draft: TransferDraft,
    destination: Option<ParentSnapshot>,
    choices: HashMap<SourceNodeId, BoundChoice>,
    cancel: &ScanCancellation,
) -> Result<TransferReview, ReviewError> {
    let source_ids: HashSet<_> = source
        .entries()
        .iter()
        .filter_map(|entry| match entry.observation() {
            SourceObservation::Ready(value) => Some(value.identity()),
            _ => None,
        })
        .collect();
    let source_directories: HashSet<_> = source
        .entries()
        .iter()
        .filter_map(|entry| match entry.observation() {
            SourceObservation::Ready(value) if value.metadata().kind == EntryKind::Directory => {
                Some(value.identity())
            }
            _ => None,
        })
        .collect();
    let destination_inside = destination.as_ref().is_some_and(|parent| {
        parent
            .ancestor_identities()
            .any(|id| source_directories.contains(&id))
    });
    let mut reservations: Reservations = HashMap::new();
    let mut review_bytes = 0usize;
    let mut probes = 0usize;
    let mut rows: Vec<ReviewRow> = Vec::with_capacity(source.entries().len());
    for entry in source.entries() {
        cancelled(cancel)?;
        review_bytes = review_bytes
            .checked_add(512)
            .filter(|n| *n <= MAX_REVIEW_BYTES)
            .ok_or(ReviewError::Limit)?;
        let id = entry.id();
        let index = rows.len();
        let choice = choices.get(&id);
        let parent_row = entry.parent().map(|parent| &rows[parent.index()]);
        if matches!(choice, Some(BoundChoice::Skip))
            || parent_row.is_some_and(|row| matches!(row.state, ReviewState::Skipped))
        {
            rows.push(ReviewRow {
                source: id,
                path: None,
                state: ReviewState::Skipped,
            });
            continue;
        }
        let observed = match entry.observation() {
            SourceObservation::Ready(source) => source,
            SourceObservation::Unavailable(_) => {
                rows.push(ReviewRow {
                    source: id,
                    path: None,
                    state: ReviewState::Blocked(ReviewBlock::SourceUnavailable),
                });
                continue;
            }
        };
        let parent = if let Some(parent_row) = parent_row {
            if let Some(parent) = parent_for_children(parent_row) {
                parent
            } else {
                if choice.is_some() {
                    return Err(ReviewError::ChoiceChanged(id));
                }
                rows.push(ReviewRow {
                    source: id,
                    path: None,
                    state: ReviewState::WaitingParent {
                        parent: parent_row.source,
                    },
                });
                continue;
            }
        } else if destination_inside {
            rows.push(ReviewRow {
                source: id,
                path: None,
                state: ReviewState::Blocked(ReviewBlock::DestinationInsideSource),
            });
            continue;
        } else {
            DestinationParent::Existing(
                destination
                    .clone()
                    .unwrap_or_else(|| observed.parent().clone()),
            )
        };
        if let Some(BoundChoice::KeepBoth {
            parent: expected, ..
        }) = choice
            && *expected != parent
        {
            return Err(ReviewError::ChoiceChanged(id));
        }
        let selected = if entry.parent().is_none()
            && matches!(&draft.action, TransferAction::Duplicate)
            && choice.is_none()
        {
            let DestinationParent::Existing(ref parent) = parent else {
                return Err(ReviewError::InvalidChoice);
            };
            match duplicate_target(parent, observed.leaf(), &reservations, cancel, &mut probes) {
                Ok(Some(target)) => Some(target),
                Ok(None) | Err(AdmissionError::InvalidLeaf) => {
                    rows.push(ReviewRow {
                        source: id,
                        path: None,
                        state: ReviewState::Blocked(ReviewBlock::DuplicateNameExhausted),
                    });
                    continue;
                }
                Err(AdmissionError::Cancelled) => return Err(ReviewError::Cancelled),
                Err(AdmissionError::Limit) => return Err(ReviewError::Limit),
                Err(error) => {
                    rows.push(ReviewRow {
                        source: id,
                        path: None,
                        state: ReviewState::Blocked(ReviewBlock::Admission(error)),
                    });
                    continue;
                }
            }
        } else {
            None
        };
        let leaf = match choice {
            Some(BoundChoice::KeepBoth { leaf, .. }) => leaf.clone(),
            _ => selected.as_ref().map_or_else(
                || observed.leaf().clone(),
                |target| target.requested_leaf().clone(),
            ),
        };
        // Charge the row path, target proof and retained native leaf copies
        // before storing them. Planned paths are also charged, even though
        // their directories do not yet exist.
        let path_units = parent
            .path()
            .as_os_str()
            .encode_wide()
            .count()
            .saturating_add(1)
            .saturating_add(leaf.as_os_str().encode_wide().count());
        let chain_bytes = match &parent {
            DestinationParent::Existing(p) => p
                .ancestor_identities()
                .count()
                .saturating_mul(std::mem::size_of::<super::DirectoryStamp>()),
            DestinationParent::Planned { .. } => 0,
        };
        let choice_path_bytes = if let Some(BoundChoice::KeepBoth { parent, .. }) = choice {
            parent.path().as_os_str().as_encoded_bytes().len()
        } else {
            0
        };
        let cost = path_units
            .saturating_mul(4)
            .saturating_add(chain_bytes)
            .saturating_add(choice_path_bytes)
            .saturating_add(1024);
        review_bytes = review_bytes
            .checked_add(cost)
            .filter(|n| *n <= MAX_REVIEW_BYTES)
            .ok_or(ReviewError::Limit)?;
        let path = parent.path().join(leaf.as_os_str());
        if let Err(error) = checked_parts(&path) {
            rows.push(ReviewRow {
                source: id,
                path: Some(path),
                state: ReviewState::Blocked(ReviewBlock::Admission(error)),
            });
            continue;
        }
        let names = reservations.entry(parent.namespace()).or_default();
        let key = OrdinalName(leaf.as_os_str().to_owned());
        if let Some(&other_index) = names.get(&key) {
            let other = rows[other_index].source;
            if choice.is_some() || choices.contains_key(&other) {
                return Err(ReviewError::ChoiceChanged(id));
            }
            rows[other_index].state =
                ReviewState::Blocked(ReviewBlock::NameCollision { other: id });
            rows.push(ReviewRow {
                source: id,
                path: Some(path),
                state: ReviewState::Blocked(ReviewBlock::NameCollision { other }),
            });
            continue;
        }
        names.insert(key, index);
        let state = match parent {
            DestinationParent::Planned { directory, .. } => {
                match choice {
                    Some(BoundChoice::KeepBoth { vacancy: None, .. }) => {}
                    None => {}
                    _ => return Err(ReviewError::ChoiceChanged(id)),
                }
                ReviewState::PlannedParent { directory, leaf }
            }
            DestinationParent::Existing(parent) => {
                let target = match selected {
                    Some(target) => Ok(target),
                    None => inspect_target(&parent, &leaf, cancel),
                };
                if let Some(choice) = choice {
                    resolve_existing_choice(id, choice, target, cancel)?
                } else {
                    match target {
                        Ok(target) => match target.occupancy() {
                            TargetOccupancy::Vacant => ReviewState::Vacant(target),
                            TargetOccupancy::Occupied(occupied)
                                if source_ids.contains(&occupied.identity()) =>
                            {
                                ReviewState::Blocked(ReviewBlock::TargetAliasesSource)
                            }
                            TargetOccupancy::Occupied(_) => ReviewState::Conflict(target),
                        },
                        Err(AdmissionError::Cancelled) => return Err(ReviewError::Cancelled),
                        Err(error) => ReviewState::Blocked(ReviewBlock::Admission(error)),
                    }
                }
            }
        };
        rows.push(ReviewRow {
            source: id,
            path: Some(path),
            state,
        });
    }
    // A later root may collide with an earlier planned directory after its
    // descendants were mapped. Retire those descendants in source order.
    for index in 0..rows.len() {
        let Some(parent) = source
            .get(rows[index].source)
            .and_then(|entry| entry.parent())
        else {
            continue;
        };
        let parent_row = &rows[parent.index()];
        if matches!(parent_row.state, ReviewState::Skipped) {
            rows[index].path = None;
            rows[index].state = ReviewState::Skipped;
        } else if !matches!(
            parent_row.state,
            ReviewState::Vacant(_) | ReviewState::PlannedParent { .. } | ReviewState::Merge { .. }
        ) && !matches!(rows[index].state, ReviewState::Skipped)
        {
            if choices.contains_key(&rows[index].source) {
                return Err(ReviewError::ChoiceChanged(rows[index].source));
            }
            rows[index].path = None;
            rows[index].state = ReviewState::WaitingParent { parent };
        }
    }
    let generation = NEXT_REVIEW
        .try_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |n| {
            n.checked_add(1)
        })
        .map_err(|_| ReviewError::GenerationExhausted)?;
    Ok(TransferReview {
        generation: ReviewGeneration(generation),
        queued: Arc::new(AtomicBool::new(false)),
        draft,
        source,
        rows,
        destination,
        choices,
    })
}

fn parent_for_children(row: &ReviewRow) -> Option<DestinationParent> {
    match &row.state {
        ReviewState::Vacant(_) | ReviewState::PlannedParent { .. } => {
            row.path.clone().map(|path| DestinationParent::Planned {
                directory: row.source,
                path,
            })
        }
        ReviewState::Merge { directory, .. } => {
            Some(DestinationParent::Existing(directory.clone()))
        }
        _ => None,
    }
}

impl From<AdmissionError> for ReviewError {
    fn from(error: AdmissionError) -> Self {
        match error {
            AdmissionError::Cancelled => Self::Cancelled,
            AdmissionError::Limit => Self::Limit,
            error => Self::Admission(error),
        }
    }
}

fn resolve_existing_choice(
    id: SourceNodeId,
    choice: &BoundChoice,
    current: Result<TargetSnapshot, AdmissionError>,
    cancel: &ScanCancellation,
) -> Result<ReviewState, ReviewError> {
    let current = current?;
    match choice {
        BoundChoice::KeepBoth {
            vacancy: Some(expect),
            ..
        } if *expect == current => Ok(ReviewState::Vacant(current)),
        BoundChoice::Replace(expect) if *expect == current => Ok(ReviewState::Replace(current)),
        BoundChoice::Merge { target, directory } if *target == current => {
            if inspect_parent(directory.path(), cancel)? != *directory {
                return Err(ReviewError::ChoiceChanged(id));
            }
            Ok(ReviewState::Merge {
                target: current,
                directory: directory.clone(),
            })
        }
        _ => Err(ReviewError::ChoiceChanged(id)),
    }
}

/// Destination authority captured by an explicitly approved review. Created
/// parent references must be resolved to the worker's exact exclusive create.
#[derive(Debug)]
pub enum ApprovedTarget {
    CreateAt(TargetSnapshot),
    CreateIn {
        parent: SourceNodeId,
        leaf: TransferLeaf,
    },
    Replace(TargetSnapshot),
    Merge {
        target: TargetSnapshot,
        directory: ParentSnapshot,
    },
}

#[derive(Debug)]
pub struct ApprovedTransferEntry {
    id: SourceNodeId,
    source: EntrySnapshot,
    destination: ApprovedTarget,
    verify_content: bool,
    undo_digest: Option<UndoDigest>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct UndoDigest {
    pub sha256: [u8; 32],
    pub zone_sha256: Option<[u8; 32]>,
}

impl ApprovedTransferEntry {
    pub fn id(&self) -> SourceNodeId {
        self.id
    }

    pub fn source(&self) -> &EntrySnapshot {
        &self.source
    }

    pub fn destination(&self) -> &ApprovedTarget {
        &self.destination
    }

    pub fn verify_content(&self) -> bool {
        self.verify_content
    }
    pub(super) fn undo_digest(&self) -> Option<UndoDigest> {
        self.undo_digest
    }
}

/// Immutable review evidence for a later worker. Publication and removal
/// require new held guards and content checks. A streamed output inherits its
/// destination directory permissions. An atomic rename preserves permissions.
#[derive(Debug)]
pub struct ApprovedTransferPlan {
    generation: ReviewGeneration,
    queued: Arc<AtomicBool>,
    draft: TransferDraft,
    source: Arc<SourceTree>,
    entries: Vec<ApprovedTransferEntry>,
    skipped: Vec<SourceNodeId>,
    recovery_origin: Option<RecoveryOrigin>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum RecoveryVerb {
    Retry,
    Undo,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct RecoveryOrigin {
    pub receipt: super::TransferReceiptId,
    pub node: u32,
    pub verb: RecoveryVerb,
}

impl ApprovedTransferPlan {
    pub(super) fn set_recovery_origin(&mut self, origin: RecoveryOrigin) {
        self.recovery_origin = Some(origin);
    }
    pub(super) fn recovery_origin(&self) -> Option<RecoveryOrigin> {
        self.recovery_origin
    }
    pub(super) fn seal_undo(&mut self, digest: UndoDigest) -> Result<(), ReviewError> {
        if self.entries.len() != 1
            || !matches!(self.draft.action, TransferAction::MoveTo(_))
            || !matches!(self.entries[0].destination, ApprovedTarget::CreateAt(_))
        {
            return Err(ReviewError::InvalidChoice);
        }
        self.entries[0].undo_digest = Some(digest);
        Ok(())
    }
    // All approvals sealed from this review share one queue admission.
    pub(super) fn claim_for_queue(&self) -> bool {
        !self.queued.swap(true, AtomicOrdering::AcqRel)
    }

    pub fn generation(&self) -> ReviewGeneration {
        self.generation
    }

    pub fn draft(&self) -> &TransferDraft {
        &self.draft
    }

    pub fn source(&self) -> &SourceTree {
        &self.source
    }

    pub fn entries(&self) -> &[ApprovedTransferEntry] {
        &self.entries
    }

    pub fn skipped(&self) -> &[SourceNodeId] {
        &self.skipped
    }
}

impl TransferReview {
    fn require_generation(&self, generation: ReviewGeneration) -> Result<(), ReviewError> {
        if generation != self.generation {
            return Err(ReviewError::StaleReview);
        }
        Ok(())
    }

    fn row(&self, id: SourceNodeId) -> Result<&ReviewRow, ReviewError> {
        self.rows.get(id.index()).ok_or(ReviewError::InvalidChoice)
    }

    fn ready(&self, id: SourceNodeId) -> Result<&EntrySnapshot, ReviewError> {
        match self.source.get(id).map(|entry| entry.observation()) {
            Some(SourceObservation::Ready(entry)) => Ok(entry),
            _ => Err(ReviewError::InvalidChoice),
        }
    }

    fn bind_keep_both(
        &self,
        row: &ReviewRow,
        leaf: TransferLeaf,
        cancel: &ScanCancellation,
    ) -> Result<BoundChoice, ReviewError> {
        if !matches!(
            row.state,
            ReviewState::Vacant(_)
                | ReviewState::Conflict(_)
                | ReviewState::PlannedParent { .. }
                | ReviewState::Replace(_)
                | ReviewState::Merge { .. }
                | ReviewState::Blocked(
                    ReviewBlock::NameCollision { .. }
                        | ReviewBlock::TargetAliasesSource
                        | ReviewBlock::Admission(_)
                        | ReviewBlock::DuplicateNameExhausted
                )
        ) {
            return Err(ReviewError::InvalidChoice);
        }
        if row
            .destination_path()
            .and_then(Path::file_name)
            .is_some_and(|prior| {
                compare_paths_windows_ordinal(Path::new(prior), Path::new(leaf.as_os_str()))
                    == Ordering::Equal
            })
        {
            return Err(ReviewError::InvalidChoice);
        }
        let entry = self
            .source
            .get(row.source)
            .ok_or(ReviewError::InvalidChoice)?;
        let source = self.ready(row.source)?;
        let parent = match entry.parent() {
            Some(parent) => {
                parent_for_children(self.row(parent)?).ok_or(ReviewError::InvalidChoice)?
            }
            None => DestinationParent::Existing(
                self.destination
                    .clone()
                    .unwrap_or_else(|| source.parent().clone()),
            ),
        };
        checked_parts(&parent.path().join(leaf.as_os_str()))?;
        let vacancy = if let DestinationParent::Existing(existing) = &parent {
            let target = inspect_target(existing, &leaf, cancel)?;
            if !matches!(target.occupancy(), TargetOccupancy::Vacant) {
                return Err(ReviewError::ChoiceChanged(row.source));
            }
            Some(target)
        } else {
            None
        };
        Ok(BoundChoice::KeepBoth {
            parent,
            leaf,
            vacancy,
        })
    }

    fn bind_conflict(
        &self,
        row: &ReviewRow,
        required: EntryKind,
        cancel: &ScanCancellation,
    ) -> Result<BoundChoice, ReviewError> {
        let ReviewState::Conflict(target) = &row.state else {
            return Err(ReviewError::InvalidChoice);
        };
        let TargetOccupancy::Occupied(occupant) = target.occupancy() else {
            return Err(ReviewError::InvalidChoice);
        };
        if self.ready(row.source)?.metadata().kind != required
            || occupant.metadata().kind != required
        {
            return Err(ReviewError::InvalidChoice);
        }
        recheck_target_metadata(target, cancel)?;
        if required == EntryKind::Directory {
            let directory = inspect_parent(&occupant.path(), cancel)?;
            if directory.identity() != occupant.identity()
                || directory.chain.split_last().map(|(_, ancestors)| ancestors)
                    != Some(target.parent().chain.as_ref())
            {
                return Err(ReviewError::ChoiceChanged(row.source));
            }
            recheck_target_metadata(target, cancel)?;
            Ok(BoundChoice::Merge {
                target: target.clone(),
                directory,
            })
        } else {
            Ok(BoundChoice::Replace(target.clone()))
        }
    }

    fn clear_branch_choices(
        &self,
        choices: &mut HashMap<SourceNodeId, BoundChoice>,
        root: SourceNodeId,
    ) {
        let mut affected = HashSet::from([root]);
        for entry in self.source.entries() {
            if affected.contains(&entry.id())
                || entry
                    .parent()
                    .is_some_and(|parent| affected.contains(&parent))
            {
                affected.insert(entry.id());
                choices.remove(&entry.id());
            }
        }
    }

    fn with_choices(
        &self,
        choices: HashMap<SourceNodeId, BoundChoice>,
        cancel: &ScanCancellation,
    ) -> Result<Self, ReviewError> {
        build_review(
            self.source.clone(),
            self.draft.clone(),
            self.destination.clone(),
            choices,
            cancel,
        )
    }

    /// Apply one explicit choice against the current visible review.
    pub fn choose(
        &self,
        generation: ReviewGeneration,
        id: SourceNodeId,
        choice: ReviewChoice,
        cancel: &ScanCancellation,
    ) -> Result<Self, ReviewError> {
        self.require_generation(generation)?;
        cancelled(cancel)?;
        let row = self.row(id)?;
        let bound = match choice {
            ReviewChoice::Reset => None,
            ReviewChoice::Skip => Some(BoundChoice::Skip),
            ReviewChoice::KeepBoth(leaf) => Some(self.bind_keep_both(row, leaf, cancel)?),
            ReviewChoice::Replace => Some(self.bind_conflict(row, EntryKind::File, cancel)?),
            ReviewChoice::Merge => Some(self.bind_conflict(row, EntryKind::Directory, cancel)?),
        };
        let mut choices = self.choices.clone();
        self.clear_branch_choices(&mut choices, id);
        if let Some(bound) = bound {
            choices.insert(id, bound);
        }
        self.with_choices(choices, cancel)
    }

    /// Apply one choice to an explicit, compatible set of occupied conflicts.
    /// The set is fixed by the current review generation and all choices are
    /// accepted together.
    pub fn choose_group(
        &self,
        generation: ReviewGeneration,
        ids: &[SourceNodeId],
        choice: GroupChoice,
        cancel: &ScanCancellation,
    ) -> Result<Self, ReviewError> {
        self.require_generation(generation)?;
        if ids.is_empty() || ids.len() > self.rows.len() {
            return Err(ReviewError::InvalidChoice);
        }
        let mut seen = HashSet::new();
        let mut class = None;
        let mut choices = self.choices.clone();
        for id in ids {
            cancelled(cancel)?;
            let row = self.row(*id)?;
            let ReviewState::Conflict(target) = &row.state else {
                return Err(ReviewError::InvalidChoice);
            };
            let TargetOccupancy::Occupied(occupant) = target.occupancy() else {
                return Err(ReviewError::InvalidChoice);
            };
            let kind = (self.ready(*id)?.metadata().kind, occupant.metadata().kind);
            if !seen.insert(*id) || class.is_some_and(|class| class != kind) {
                return Err(ReviewError::InvalidChoice);
            }
            class = Some(kind);
            let bound = match choice {
                GroupChoice::Skip => BoundChoice::Skip,
                GroupChoice::Replace => self.bind_conflict(row, EntryKind::File, cancel)?,
            };
            choices.insert(*id, bound);
        }
        self.with_choices(choices, cancel)
    }

    /// Seal the exact currently reviewed source and destination observations
    /// after one explicit approval. The later worker rechecks under held guards.
    pub fn approve(
        &self,
        generation: ReviewGeneration,
        cancel: &ScanCancellation,
    ) -> Result<ApprovedTransferPlan, ReviewError> {
        self.require_generation(generation)?;
        if let Some(unresolved) = self.rows.iter().find(|row| {
            matches!(
                row.state,
                ReviewState::Conflict(_)
                    | ReviewState::WaitingParent { .. }
                    | ReviewState::Blocked(_)
            )
        }) {
            return Err(ReviewError::Unresolved(unresolved.source));
        }

        let mut entries = Vec::new();
        let mut skipped = Vec::new();
        let mut replaced_objects = HashSet::new();
        for row in &self.rows {
            cancelled(cancel)?;
            let destination = match &row.state {
                ReviewState::Vacant(target) => {
                    recheck_target_metadata(target, cancel)?;
                    ApprovedTarget::CreateAt(target.clone())
                }
                ReviewState::PlannedParent { directory, leaf } => ApprovedTarget::CreateIn {
                    parent: *directory,
                    leaf: leaf.clone(),
                },
                ReviewState::Replace(target) => {
                    recheck_target_metadata(target, cancel)?;
                    let TargetOccupancy::Occupied(occupant) = target.occupancy() else {
                        return Err(ReviewError::InvalidChoice);
                    };
                    if !replaced_objects.insert(occupant.identity()) {
                        return Err(ReviewError::ChoiceChanged(row.source));
                    }
                    ApprovedTarget::Replace(target.clone())
                }
                ReviewState::Merge { target, directory } => {
                    recheck_target_metadata(target, cancel)?;
                    if inspect_parent(directory.path(), cancel)? != *directory {
                        return Err(ReviewError::ChoiceChanged(row.source));
                    }
                    ApprovedTarget::Merge {
                        target: target.clone(),
                        directory: directory.clone(),
                    }
                }
                ReviewState::Skipped => {
                    skipped.push(row.source);
                    continue;
                }
                _ => return Err(ReviewError::Unresolved(row.source)),
            };
            let source = self.ready(row.source)?;
            recheck_source_metadata(source, cancel)?;
            entries.push(ApprovedTransferEntry {
                id: row.source,
                source: source.clone(),
                destination,
                verify_content: self.draft.verify
                    || matches!(self.draft.action, TransferAction::MoveTo(_)),
                undo_digest: None,
            });
        }
        if entries.is_empty() {
            return Err(ReviewError::NothingToTransfer);
        }
        Ok(ApprovedTransferPlan {
            generation: self.generation,
            queued: self.queued.clone(),
            draft: self.draft.clone(),
            source: self.source.clone(),
            entries,
            skipped,
            recovery_origin: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_transfer::{SourceTreeEntry, inspect_source_tree};
    use std::fs;
    fn tree(paths: &[PathBuf], cancel: &ScanCancellation) -> Arc<SourceTree> {
        Arc::new(inspect_source_tree(paths, cancel).unwrap())
    }
    fn ready(entry: &SourceTreeEntry) -> &super::super::EntrySnapshot {
        match entry.observation() {
            SourceObservation::Ready(value) => value,
            _ => panic!("expected admitted source"),
        }
    }

    #[test]
    fn transfer_review_replace_binds_the_incumbent_and_refuses_a_late_arrival() {
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("report.txt");
        let destination = temp.path().join("dest");
        fs::create_dir(&destination).unwrap();
        fs::write(&source_path, b"source").unwrap();
        let occupant = destination.join("report.txt");
        fs::write(&occupant, b"reviewed incumbent").unwrap();
        let cancel = ScanCancellation::new();
        let review = review_transfer(
            tree(&[source_path.clone()], &cancel),
            TransferDraft {
                action: TransferAction::CopyTo(destination.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let id = review.rows()[0].source();
        assert!(
            matches!(review.approve(review.generation(), &cancel), Err(ReviewError::Unresolved(node)) if node == id)
        );
        let decided = review
            .choose(review.generation(), id, ReviewChoice::Replace, &cancel)
            .unwrap();
        assert!(matches!(
            decided.approve(review.generation(), &cancel),
            Err(ReviewError::StaleReview)
        ));
        let plan = decided.approve(decided.generation(), &cancel).unwrap();
        let ApprovedTarget::Replace(target) = plan.entries()[0].destination() else {
            panic!("replacement was not sealed");
        };
        assert!(
            matches!(target.occupancy(), TargetOccupancy::Occupied(entry) if entry.metadata().byte_len == Some(18))
        );
        assert_eq!(fs::read(&occupant).unwrap(), b"reviewed incumbent");

        fs::rename(&occupant, destination.join("earlier.txt")).unwrap();
        fs::write(&occupant, b"foreign arrival").unwrap();
        assert!(matches!(
            decided.approve(decided.generation(), &cancel),
            Err(ReviewError::Admission(AdmissionError::StaleTarget))
        ));
        assert_eq!(fs::read(&source_path).unwrap(), b"source");
        assert_eq!(fs::read(&occupant).unwrap(), b"foreign arrival");
    }

    #[test]
    fn transfer_review_merge_requires_an_independent_choice_for_every_child_conflict() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source").join("Notes");
        let destination = temp.path().join("dest");
        let existing_folder = destination.join("NOTES");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&existing_folder).unwrap();
        fs::write(source.join("memo.md"), b"new").unwrap();
        fs::write(existing_folder.join("MEMO.MD"), b"original").unwrap();

        let cancel = ScanCancellation::new();
        let review = review_transfer(
            tree(&[source], &cancel),
            TransferDraft {
                action: TransferAction::CopyTo(destination),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        let folder = review.rows()[0].source();
        let memo = review.rows()[1].source();
        assert!(matches!(
            review.choose(review.generation(), folder, ReviewChoice::Replace, &cancel),
            Err(ReviewError::InvalidChoice)
        ));
        let review = review
            .choose(review.generation(), folder, ReviewChoice::Merge, &cancel)
            .unwrap();
        assert!(matches!(
            review.rows()[0].state(),
            ReviewState::Merge { .. }
        ));
        assert!(matches!(review.rows()[1].state(), ReviewState::Conflict(_)));
        assert!(
            matches!(review.approve(review.generation(), &cancel), Err(ReviewError::Unresolved(id)) if id == memo)
        );

        let renamed = TransferLeaf::new("memo (copy).md".as_ref()).unwrap();
        let review = review
            .choose(
                review.generation(),
                memo,
                ReviewChoice::KeepBoth(renamed.clone()),
                &cancel,
            )
            .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        assert!(
            matches!(plan.entries()[0].destination(), ApprovedTarget::Merge { directory, .. }
            if directory.path() == existing_folder)
        );
        assert!(
            matches!(plan.entries()[1].destination(), ApprovedTarget::CreateAt(target)
            if target.requested_leaf() == &renamed && matches!(target.occupancy(), TargetOccupancy::Vacant))
        );
        assert_eq!(
            fs::read(existing_folder.join("MEMO.MD")).unwrap(),
            b"original"
        );
        assert!(!existing_folder.join(renamed.as_os_str()).exists());

        let review = review
            .choose(review.generation(), folder, ReviewChoice::Reset, &cancel)
            .unwrap();
        assert!(matches!(
            review.rows()[1].state(),
            ReviewState::WaitingParent { .. }
        ));
        let reopened = review
            .choose(review.generation(), folder, ReviewChoice::Merge, &cancel)
            .unwrap();
        assert!(matches!(
            reopened.rows()[1].state(),
            ReviewState::Conflict(_)
        ));
    }

    #[test]
    fn transfer_review_keep_both_reprobes_vacancy_and_resolves_a_case_collision() {
        let temp = tempfile::tempdir().unwrap();
        let left = temp.path().join("left");
        let right = temp.path().join("right");
        let dest = temp.path().join("dest");
        for path in [&left, &right, &dest] {
            fs::create_dir(path).unwrap();
        }
        let original_a = left.join("file.txt");
        let original_b = right.join("FILE.TXT");
        fs::write(&original_a, b"A").unwrap();
        fs::write(&original_b, b"B").unwrap();
        let cancel = ScanCancellation::new();
        let review = review_transfer(
            tree(&[original_a, original_b], &cancel),
            TransferDraft {
                action: TransferAction::CopyTo(dest.clone()),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        let id = review.rows()[1].source();
        fs::write(dest.join("alternate.txt"), b"competing file").unwrap();
        let alternate = TransferLeaf::new("ALTERNATE.TXT".as_ref()).unwrap();
        assert!(matches!(
            review.choose(review.generation(), id, ReviewChoice::KeepBoth(alternate), &cancel),
            Err(ReviewError::ChoiceChanged(node)) if node == id
        ));
        let new_leaf = TransferLeaf::new("other.txt".as_ref()).unwrap();
        let review = review
            .choose(
                review.generation(),
                id,
                ReviewChoice::KeepBoth(new_leaf),
                &cancel,
            )
            .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        assert_eq!(plan.entries().len(), 2);
        assert!(
            plan.entries()
                .iter()
                .all(|entry| matches!(entry.destination(), ApprovedTarget::CreateAt(_)))
        );
        assert_eq!(
            fs::read(dest.join("alternate.txt")).unwrap(),
            b"competing file"
        );
        assert!(!dest.join("other.txt").exists());
    }

    #[test]
    fn transfer_review_group_replace_stays_within_the_reviewed_explicit_set() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input");
        let output = temp.path().join("output");
        fs::create_dir(&input).unwrap();
        fs::create_dir(&output).unwrap();
        let names = ["a.txt", "b.txt", "unselected.txt"];
        let mut inputs = Vec::new();
        for name in names {
            let path = input.join(name);
            fs::write(&path, b"copy").unwrap();
            fs::write(output.join(name), b"original").unwrap();
            inputs.push(path);
        }
        let cancel = ScanCancellation::new();
        let review = review_transfer(
            tree(&inputs, &cancel),
            TransferDraft {
                action: TransferAction::CopyTo(output.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let ids: Vec<_> = review.rows().iter().map(ReviewRow::source).collect();
        assert!(matches!(
            review.choose_group(
                review.generation(),
                &[ids[0], ids[0]],
                GroupChoice::Replace,
                &cancel
            ),
            Err(ReviewError::InvalidChoice)
        ));
        let review = review
            .choose_group(
                review.generation(),
                &ids[..2],
                GroupChoice::Replace,
                &cancel,
            )
            .unwrap();
        assert!(matches!(review.rows()[2].state(), ReviewState::Conflict(_)));
        let review = review
            .choose(review.generation(), ids[2], ReviewChoice::Skip, &cancel)
            .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        assert_eq!(plan.skipped(), &[ids[2]]);
        assert_eq!(
            plan.entries()
                .iter()
                .map(ApprovedTransferEntry::id)
                .collect::<Vec<_>>(),
            &ids[..2],
        );
        assert!(
            plan.entries()
                .iter()
                .all(|entry| matches!(entry.destination(), ApprovedTarget::Replace(_)))
        );
        for name in names {
            assert_eq!(fs::read(output.join(name)).unwrap(), b"original");
        }
    }

    #[test]
    fn transfer_review_keep_both_does_not_follow_a_reproposed_duplicate_parent() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Folder");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("child.txt"), b"source").unwrap();
        let cancel = ScanCancellation::new();
        let review = review_transfer(
            tree(&[source], &cancel),
            TransferDraft {
                action: TransferAction::Duplicate,
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        let proposed = review.rows()[0].destination_path().unwrap();
        fs::create_dir(proposed).unwrap();
        fs::write(proposed.join("foreign.txt"), b"arrived late").unwrap();

        let child = review.rows()[1].source();
        let alternate = TransferLeaf::new("alternate.txt".as_ref()).unwrap();
        assert!(matches!(
            review.choose(review.generation(), child, ReviewChoice::KeepBoth(alternate), &cancel),
            Err(ReviewError::ChoiceChanged(id)) if id == child
        ));
        assert_eq!(
            fs::read(proposed.join("foreign.txt")).unwrap(),
            b"arrived late"
        );
        assert!(!temp.path().join("Folder (copy 2)").exists());
    }

    #[test]
    fn transfer_review_long_duplicate_name_accepts_an_explicit_shorter_leaf() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join(format!("{}.txt", "x".repeat(251)));
        fs::write(&source, b"maximum-length Windows leaf").unwrap();
        let cancel = ScanCancellation::new();
        let review = review_transfer(
            tree(&[source], &cancel),
            TransferDraft {
                action: TransferAction::Duplicate,
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        assert!(matches!(
            review.rows()[0].state(),
            ReviewState::Blocked(ReviewBlock::DuplicateNameExhausted)
        ));
        let leaf = TransferLeaf::new("shorter.txt".as_ref()).unwrap();
        let id = review.rows()[0].source();
        let review = review
            .choose(
                review.generation(),
                id,
                ReviewChoice::KeepBoth(leaf.clone()),
                &cancel,
            )
            .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        assert!(
            matches!(plan.entries()[0].destination(), ApprovedTarget::CreateAt(target)
            if target.requested_leaf() == &leaf && matches!(target.occupancy(), TargetOccupancy::Vacant))
        );
        assert!(!temp.path().join(leaf.as_os_str()).exists());
    }

    #[test]
    fn transfer_review_duplicate_is_vacant_and_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("file.txt");
        fs::write(&source, b"unchanged").unwrap();
        let cancel = ScanCancellation::new();
        let snapshot = tree(std::slice::from_ref(&source), &cancel);
        let review = review_transfer(
            snapshot,
            TransferDraft {
                action: TransferAction::Duplicate,
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        assert!(matches!(review.rows()[0].state(), ReviewState::Vacant(_)));
        assert_eq!(
            review.rows()[0]
                .destination_path()
                .unwrap()
                .file_name()
                .unwrap(),
            "file (copy).txt"
        );
        assert!(!review.rows()[0].destination_path().unwrap().exists());
        assert_eq!(fs::read(source).unwrap(), b"unchanged");
    }
    #[test]
    fn transfer_review_new_directory_has_planned_children() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("picked");
        let nested = source.join("nested");
        let dest = temp.path().join("dest");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir(&dest).unwrap();
        fs::write(nested.join("file.txt"), b"x").unwrap();
        let cancel = ScanCancellation::new();
        let snapshot = tree(std::slice::from_ref(&source), &cancel);
        let review = review_transfer(
            snapshot.clone(),
            TransferDraft {
                action: TransferAction::CopyTo(dest.clone()),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        assert!(matches!(review.rows()[0].state(), ReviewState::Vacant(_)));
        assert!(
            matches!(review.rows()[1].state(),ReviewState::PlannedParent { directory,.. } if *directory==snapshot.entries()[0].id())
        );
        assert!(
            matches!(review.rows()[2].state(),ReviewState::PlannedParent { directory,.. } if *directory==snapshot.entries()[1].id())
        );
        assert!(!dest.join("picked").exists());
    }
    #[test]
    fn transfer_review_blocks_descendant_and_selected_hardlink_alias() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let nested = source.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let file = source.join("one.txt");
        fs::write(&file, b"x").unwrap();
        let cancel = ScanCancellation::new();
        let snapshot = tree(std::slice::from_ref(&source), &cancel);
        let review = review_transfer(
            snapshot,
            TransferDraft {
                action: TransferAction::CopyTo(nested),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        assert!(matches!(
            review.rows()[0].state(),
            ReviewState::Blocked(ReviewBlock::DestinationInsideSource)
        ));
        let dest = temp.path().join("dest");
        fs::create_dir(&dest).unwrap();
        fs::hard_link(&file, dest.join("one.txt")).unwrap();
        let snapshot = tree(std::slice::from_ref(&file), &cancel);
        assert_eq!(ready(&snapshot.entries()[0]).path(), file);
        let review = review_transfer(
            snapshot,
            TransferDraft {
                action: TransferAction::MoveTo(dest),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        assert!(matches!(
            review.rows()[0].state(),
            ReviewState::Blocked(ReviewBlock::TargetAliasesSource)
        ));
    }
    #[test]
    fn transfer_review_case_collision_blocks_both_rows_and_descendants() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        let dest = temp.path().join("dest");
        fs::create_dir_all(a.join("Folder")).unwrap();
        fs::create_dir_all(b.join("folder")).unwrap();
        fs::create_dir(&dest).unwrap();
        fs::write(a.join("Folder").join("child"), b"x").unwrap();
        let cancel = ScanCancellation::new();
        let snapshot = tree(&[a.join("Folder"), b.join("folder")], &cancel);
        let review = review_transfer(
            snapshot,
            TransferDraft {
                action: TransferAction::CopyTo(dest),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        assert!(matches!(
            review.rows()[0].state(),
            ReviewState::Blocked(ReviewBlock::NameCollision { .. })
        ));
        assert!(matches!(
            review.rows()[1].state(),
            ReviewState::WaitingParent { .. }
        ));
        assert!(matches!(
            review.rows()[2].state(),
            ReviewState::Blocked(ReviewBlock::NameCollision { .. })
        ));
    }
    #[test]
    fn transfer_review_existing_case_conflict_waits_for_merge() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source").join("Folder");
        let destination = temp.path().join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("child"), b"x").unwrap();
        fs::create_dir_all(destination.join("folder")).unwrap();
        let cancel = ScanCancellation::new();
        let snapshot = tree(std::slice::from_ref(&source), &cancel);
        let review = review_transfer(
            snapshot,
            TransferDraft {
                action: TransferAction::CopyTo(destination),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        assert!(
            matches!(review.rows()[0].state(),ReviewState::Conflict(target) if matches!(target.occupancy(),TargetOccupancy::Occupied(_)))
        );
        assert!(matches!(
            review.rows()[1].state(),
            ReviewState::WaitingParent { .. }
        ));
    }
}
