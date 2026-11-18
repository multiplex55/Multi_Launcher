//! Versioned, data-only transfer receipts. Decoding conveys evidence, not
//! permission to operate on a path; any later action requires fresh admission.
use super::frames::MAX_FRAME_BYTES;
use crate::file_operations::NativeFileIdentity;
use crate::file_transfer::{EntryKind, TransferLeaf};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

const MAX_NODES: usize = 10_000;
const MAX_PARENTS: usize = 100_000;
const MAX_DEPTH: usize = 128;
const MAX_UNITS: usize = 32_766;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) struct ReceiptId(pub [u8; 16]);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) struct ParentId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) struct NodeId(pub u32);
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) struct Name(pub Vec<u16>);
impl Name {
    pub(super) fn valid(&self) -> bool {
        if self.0.is_empty() || self.0.len() > 255 {
            return false;
        }
        TransferLeaf::new(&OsString::from_wide(&self.0)).is_ok()
    }
    fn same_slot(&self, other: &Name) -> bool {
        let left = OsString::from_wide(&self.0);
        let right = OsString::from_wide(&other.0);
        crate::batch_rename::compare_paths_windows_ordinal(Path::new(&left), Path::new(&right))
            == std::cmp::Ordering::Equal
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) struct Place {
    pub parent: ParentId,
    pub leaf: Name,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Object {
    pub place: Place,
    pub identity: NativeFileIdentity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Kind {
    File,
    Directory,
}
impl From<EntryKind> for Kind {
    fn from(value: EntryKind) -> Self {
        match value {
            EntryKind::File => Self::File,
            EntryKind::Directory => Self::Directory,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Revision {
    pub kind: Kind,
    pub length: Option<u64>,
    pub attributes: u32,
    pub created: i64,
    pub modified: i64,
    pub changed: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum ParentOrigin {
    Root(Vec<u16>),
    Child(Place),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ParentProof {
    pub id: ParentId,
    pub origin: ParentOrigin,
    pub identity: NativeFileIdentity,
    pub created: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Action {
    Copy,
    Move,
    Duplicate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum PlannedTarget {
    Direct { place: Place, policy: TargetPolicy },
    Child { parent: NodeId, leaf: Name },
    Skipped,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum TargetPolicy {
    Vacant,
    Replace {
        incumbent: Object,
        revision: Revision,
    },
    Merge {
        incumbent: Object,
        revision: Revision,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct OutlineNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub source: Option<Object>,
    pub revision: Option<Revision>,
    pub target: PlannedTarget,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Binding {
    pub node: NodeId,
    pub target: Place,
    pub incumbent: Option<Object>,
    pub incumbent_revision: Option<Revision>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Verification {
    NotRequested,
    Verified {
        sha256: [u8; 32],
        zone_sha256: Option<[u8; 32]>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum SourceAction {
    Relocate,
    Dispose,
    Restore,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum SourceState {
    Original,
    Quarantined,
    Removed,
    Uncertain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum AtomicSourceDisposition {
    Vacant,
    Foreign(NativeFileIdentity),
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Removal {
    Removed,
    Replaced(NativeFileIdentity),
    Retained,
    Uncertain,
}

/// Each variant corresponds to one existing journal effect. Intents are
/// durable before mutation; observations never imply a future action is safe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Effect {
    StageIntent {
        stage: Place,
    },
    StageCreated {
        stage: Object,
    },
    DisplacementIntent {
        original: Object,
        backup: Place,
    },
    DisplacementObserved {
        original: Object,
        backup: Place,
    },
    PublishIntent {
        stage: Object,
        target: Place,
    },
    PublicationObserved {
        output: Object,
    },
    Verified {
        output: Object,
        verification: Verification,
        bytes: u64,
        revision: Revision,
    },
    RestoreIntent {
        original: Object,
        backup: Place,
    },
    RestoreObserved {
        original: Object,
        backup: Place,
    },
    MoveRenameIntent {
        source: Object,
        target: Place,
        original: Revision,
    },
    MoveRenameObserved {
        source: Object,
        target: Place,
        at_target: Option<bool>,
        source_disposition: AtomicSourceDisposition,
    },
    MoveMetadataIntent {
        output: Object,
    },
    MoveMetadataObserved {
        output: Object,
        revision: Revision,
    },
    MoveVerified {
        output: Object,
        revision: Revision,
        verification: Verification,
    },
    StagedSourceIntent {
        action: SourceAction,
        source: Object,
        recovery: Option<Place>,
        state: SourceState,
        output: Object,
    },
    StagedSourceObserved {
        action: SourceAction,
        source: Object,
        recovery: Option<Place>,
        state: SourceState,
        output: Object,
    },
    DirectoryCreateIntent {
        target: Place,
    },
    DirectoryCreated {
        output: Object,
        parent_proof: ParentId,
    },
    DirectoryMetadataIntent {
        output: Object,
    },
    DirectoryMetadataObserved {
        output: Object,
        revision: Revision,
    },
    DirectoryRemoveIntent {
        source: Object,
        destination: ParentId,
    },
    DirectoryRemoveObserved {
        source: Object,
        destination: ParentId,
        observation: Removal,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::file_transfer) enum EntryOutcome {
    Completed,
    Skipped,
    RetainedByReview,
    Failed,
    Cancelled,
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::file_transfer) enum Disposition {
    NoEffect,
    Settled,
    Retained,
    Uncertain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::file_transfer) enum JobOutcome {
    Completed,
    Partial,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Record {
    Header {
        id: ReceiptId,
        action: Action,
        verification_requested: bool,
    },
    Parent(ParentProof),
    Node(OutlineNode),
    Bind(Binding),
    Effect {
        node: NodeId,
        effect: Effect,
    },
    EntryTerminal {
        node: NodeId,
        outcome: EntryOutcome,
        disposition: Disposition,
    },
    Finished {
        outcome: JobOutcome,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SemanticProblem {
    InvalidEncoding,
    InvalidRecord,
    MissingHeader,
    InvalidReference,
    Duplicate,
    InvalidOrder,
    Mismatch,
    Limit,
}
#[derive(Clone, Default)]
pub(super) struct State {
    pub(super) binding: Option<Binding>,
    stage: Option<Object>,
    stage_intent: Option<Place>,
    backup: Option<(Object, Place)>,
    output: Option<Object>,
    pending: Option<Pending>,
    verified: bool,
    published_verified: bool,
    recovery: Option<Place>,
    source_state: Option<SourceState>,
    terminal: Option<(EntryOutcome, Disposition)>,
    source_unsettled: bool,
    lane: Option<OutputLane>,
    directory_times: bool,
    directory_removal: Option<Removal>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputLane {
    Staged,
    Atomic,
    Directory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Stage,
    Displace,
    Publish,
    Restore,
    Move,
    MoveMetadata,
    Staged(SourceAction),
    DirectoryCreate,
    DirectoryMetadata,
    DirectoryRemove,
}
#[derive(Default)]
pub(super) struct SemanticPrefix {
    pub records: Vec<Record>,
    pub problem: Option<SemanticProblem>,
    header: Option<(ReceiptId, Action)>,
    pub(super) parents: Vec<ParentProof>,
    depths: Vec<usize>,
    path_units: Vec<usize>,
    pub(super) parent_keys: HashMap<(Option<ParentId>, Vec<u16>, NativeFileIdentity), ParentId>,
    pub(super) nodes: Vec<OutlineNode>,
    pub(super) states: Vec<State>,
    finished: bool,
}
impl SemanticPrefix {
    pub fn push_bytes(&mut self, bytes: &[u8]) -> bool {
        if self.problem.is_some() {
            return false;
        }
        if bytes.len() > MAX_FRAME_BYTES {
            self.problem = Some(SemanticProblem::Limit);
            return false;
        }
        match serde_json::from_slice::<Record>(bytes) {
            Ok(record) => self.push(record),
            Err(_) => {
                self.problem = Some(SemanticProblem::InvalidEncoding);
                false
            }
        }
    }
    pub fn push(&mut self, record: Record) -> bool {
        if self.problem.is_some() {
            return false;
        }
        match self.validate(&record) {
            Ok(()) => {
                self.records.push(record);
                true
            }
            Err(error) => {
                self.problem = Some(error);
                false
            }
        }
    }
    /// A prefix with unresolved namespace intents is never called settled.
    pub fn settled(&self) -> bool {
        self.problem.is_none()
            && self.finished
            && self.states.iter().all(|s| {
                s.terminal.is_some()
                    && s.pending.is_none()
                    && s.stage.is_none()
                    && s.backup.is_none()
                    && !s.source_unsettled
                    && !matches!(
                        s.terminal,
                        Some((_, Disposition::Retained | Disposition::Uncertain))
                    )
            })
    }
    /// Conservative projection for terminal reporting; engine errors may
    /// require the caller to choose a more cautious disposition.
    pub(super) fn suggested_disposition(&self, node: NodeId) -> Option<Disposition> {
        let state = self.states.get(node.0 as usize)?;
        if state.pending.is_some() || state.source_unsettled {
            return Some(Disposition::Uncertain);
        }
        if state.stage.is_some() || state.backup.is_some() {
            return Some(Disposition::Retained);
        }
        if state.output.is_some() {
            if !state.verified && state.lane != Some(OutputLane::Directory) {
                return Some(Disposition::Retained);
            }
            if self.header.map(|h| h.1) == Some(Action::Move)
                && ((state.lane == Some(OutputLane::Staged)
                    && state.source_state != Some(SourceState::Removed))
                    || (state.lane == Some(OutputLane::Directory)
                        && !matches!(
                            state.directory_removal,
                            Some(Removal::Removed | Removal::Replaced(_))
                        )))
            {
                return Some(Disposition::Retained);
            }
            return Some(Disposition::Settled);
        }
        Some(Disposition::NoEffect)
    }
    pub(super) fn parent(&self, id: ParentId) -> Result<&ParentProof, SemanticProblem> {
        self.parents
            .get(id.0 as usize)
            .ok_or(SemanticProblem::InvalidReference)
    }
    pub(super) fn parent_path(&self, mut id: ParentId) -> Result<Vec<u16>, SemanticProblem> {
        let mut components = Vec::new();
        loop {
            match &self.parent(id)?.origin {
                ParentOrigin::Root(root) => {
                    let mut path = root.clone();
                    for component in components.into_iter().rev() {
                        if path.last() != Some(&(b'\\' as u16)) {
                            path.push(b'\\' as u16);
                        }
                        path.extend(component);
                    }
                    return Ok(path);
                }
                ParentOrigin::Child(place) => {
                    components.push(place.leaf.0.as_slice());
                    id = place.parent;
                }
            }
        }
    }
    pub(super) fn parent_snapshot(
        &self,
        mut id: ParentId,
    ) -> Result<crate::file_transfer::ParentSnapshot, SemanticProblem> {
        use std::sync::Arc;
        let path = std::path::PathBuf::from(OsString::from_wide(&self.parent_path(id)?));
        let mut chain = Vec::new();
        loop {
            let proof = self.parent(id)?;
            chain.push(crate::file_transfer::DirectoryStamp {
                identity: proof.identity,
                created: proof.created,
            });
            match &proof.origin {
                ParentOrigin::Root(_) => break,
                ParentOrigin::Child(place) => id = place.parent,
            }
        }
        chain.reverse();
        Ok(crate::file_transfer::ParentSnapshot {
            path: Arc::from(path),
            chain: chain.into(),
        })
    }
    pub(super) fn pending(&self, node: NodeId) -> bool {
        self.states
            .get(node.0 as usize)
            .is_none_or(|s| s.pending.is_some() || s.source_unsettled)
    }
    fn place(&self, place: &Place) -> Result<(), SemanticProblem> {
        self.parent(place.parent)?;
        let index = place.parent.0 as usize;
        let separator = usize::from(self.depths[index] != 0);
        let units = self.path_units[index]
            .checked_add(separator)
            .and_then(|n| n.checked_add(place.leaf.0.len()))
            .ok_or(SemanticProblem::Limit)?;
        if !place.leaf.valid() || units > MAX_UNITS {
            return Err(SemanticProblem::InvalidRecord);
        }
        Ok(())
    }
    fn object(&self, obj: &Object) -> Result<(), SemanticProblem> {
        self.place(&obj.place)?;
        if obj.identity.file_id() == [0; 16]
            || obj.identity.volume_serial_number()
                != self
                    .parent(obj.place.parent)?
                    .identity
                    .volume_serial_number()
        {
            return Err(SemanticProblem::Mismatch);
        }
        Ok(())
    }
    fn validate(&mut self, record: &Record) -> Result<(), SemanticProblem> {
        if self.finished {
            return Err(SemanticProblem::InvalidOrder);
        }
        match record {
            Record::Header { id, action, .. } => {
                if self.header.is_some() || id.0 == [0; 16] || !self.records.is_empty() {
                    return Err(SemanticProblem::InvalidOrder);
                }
                self.header = Some((*id, *action));
            }
            _ if self.header.is_none() => return Err(SemanticProblem::MissingHeader),
            Record::Parent(proof) => {
                if proof.id.0 as usize != self.parents.len()
                    || self.parents.len() >= MAX_PARENTS
                    || proof.identity.file_id() == [0; 16]
                {
                    return Err(SemanticProblem::Limit);
                }
                let (parent, leaf, units, depth) = match &proof.origin {
                    ParentOrigin::Root(path) => {
                        let raw = OsString::from_wide(path);
                        let p = Path::new(&raw);
                        if path.len() != 3
                            || path[1] != b':' as u16
                            || path[2] != b'\\' as u16
                            || !p.is_absolute()
                        {
                            return Err(SemanticProblem::InvalidRecord);
                        }
                        (None, path.clone(), path.len(), 0)
                    }
                    ParentOrigin::Child(place) => {
                        self.place(place)?;
                        let parent = self.parent(place.parent)?;
                        if parent.identity.volume_serial_number()
                            != proof.identity.volume_serial_number()
                        {
                            return Err(SemanticProblem::Mismatch);
                        }
                        let index = place.parent.0 as usize;
                        let depth = self.depths[index] + 1;
                        let separator = usize::from(self.depths[index] != 0);
                        let units = self.path_units[index]
                            .checked_add(separator)
                            .and_then(|v| v.checked_add(place.leaf.0.len()))
                            .ok_or(SemanticProblem::Limit)?;
                        (Some(place.parent), place.leaf.0.clone(), units, depth)
                    }
                };
                if units > MAX_UNITS || depth > MAX_DEPTH {
                    return Err(SemanticProblem::Limit);
                }
                let key = (parent, leaf, proof.identity);
                if self.parent_keys.contains_key(&key) {
                    return Err(SemanticProblem::Duplicate);
                }
                self.parent_keys.insert(key, proof.id);
                self.parents.push(proof.clone());
                self.depths.push(depth);
                self.path_units.push(units);
            }
            Record::Node(node) => {
                if self.nodes.len() >= MAX_NODES || node.id.0 as usize != self.nodes.len() {
                    return Err(SemanticProblem::Limit);
                }
                match (&node.source, &node.revision, &node.target) {
                    (Some(source), Some(_), _) => self.object(source)?,
                    (None, None, PlannedTarget::Skipped) => {}
                    _ => return Err(SemanticProblem::Mismatch),
                }
                if let Some(parent) = node.parent {
                    if parent.0 as usize >= self.nodes.len()
                        || self.nodes[parent.0 as usize].revision.map(|r| r.kind)
                            != Some(Kind::Directory)
                    {
                        return Err(SemanticProblem::InvalidReference);
                    }
                }
                match &node.target {
                    PlannedTarget::Direct { place, policy } => {
                        self.place(place)?;
                        let occupant = match policy {
                            TargetPolicy::Vacant => None,
                            TargetPolicy::Replace {
                                incumbent,
                                revision,
                            } => Some((incumbent, revision, Kind::File)),
                            TargetPolicy::Merge {
                                incumbent,
                                revision,
                            } => Some((incumbent, revision, Kind::Directory)),
                        };
                        if let Some((incumbent, revision, kind)) = occupant {
                            self.object(incumbent)?;
                            if node.revision.map(|revision| revision.kind) != Some(kind)
                                || revision.kind != kind
                                || incumbent.place.parent != place.parent
                                || !incumbent.place.leaf.same_slot(&place.leaf)
                            {
                                return Err(SemanticProblem::Mismatch);
                            }
                        }
                    }
                    PlannedTarget::Child { parent, leaf } => {
                        if parent.0 as usize >= self.nodes.len()
                            || node.parent != Some(*parent)
                            || self.nodes[parent.0 as usize].revision.map(|r| r.kind)
                                != Some(Kind::Directory)
                            || !leaf.valid()
                        {
                            return Err(SemanticProblem::InvalidReference);
                        }
                    }
                    PlannedTarget::Skipped => {}
                }
                self.nodes.push(node.clone());
                self.states.push(State::default());
            }
            Record::Bind(binding) => {
                self.place(&binding.target)?;
                if let Some(obj) = &binding.incumbent {
                    self.object(obj)?;
                    if obj.place.parent != binding.target.parent {
                        return Err(SemanticProblem::Mismatch);
                    }
                }
                let node = self
                    .nodes
                    .get(binding.node.0 as usize)
                    .ok_or(SemanticProblem::InvalidReference)?;
                match &node.target {
                    PlannedTarget::Direct { place, policy } if *place == binding.target => {
                        match policy {
                            TargetPolicy::Vacant
                                if binding.incumbent.is_none()
                                    && binding.incumbent_revision.is_none() => {}
                            TargetPolicy::Replace {
                                incumbent,
                                revision,
                            }
                            | TargetPolicy::Merge {
                                incumbent,
                                revision,
                            } if binding.incumbent.as_ref() == Some(incumbent)
                                && binding.incumbent_revision == Some(*revision) => {}
                            _ => return Err(SemanticProblem::Mismatch),
                        }
                    }
                    PlannedTarget::Child { parent, leaf }
                        if *leaf == binding.target.leaf
                            && binding.incumbent.is_none()
                            && binding.incumbent_revision.is_none() =>
                    {
                        let owner = self
                            .states
                            .get(parent.0 as usize)
                            .ok_or(SemanticProblem::InvalidReference)?;
                        let directory = owner
                            .output
                            .as_ref()
                            .or_else(|| owner.binding.as_ref().and_then(|b| b.incumbent.as_ref()))
                            .ok_or(SemanticProblem::InvalidReference)?;
                        let proof = self.parent(binding.target.parent)?;
                        if proof.identity != directory.identity
                            || proof.origin != ParentOrigin::Child(directory.place.clone())
                        {
                            return Err(SemanticProblem::Mismatch);
                        }
                    }
                    _ => return Err(SemanticProblem::Mismatch),
                }
                let state = &mut self.states[binding.node.0 as usize];
                if state.binding.is_some() {
                    return Err(SemanticProblem::Duplicate);
                }
                state.binding = Some(binding.clone());
            }
            Record::Effect { node, effect } => self.effect(*node, effect)?,
            Record::EntryTerminal {
                node,
                outcome,
                disposition,
            } => {
                let outline = self
                    .nodes
                    .get(node.0 as usize)
                    .ok_or(SemanticProblem::InvalidReference)?;
                let action = self.header.ok_or(SemanticProblem::MissingHeader)?.1;
                let s = self
                    .states
                    .get_mut(node.0 as usize)
                    .ok_or(SemanticProblem::InvalidReference)?;
                if s.terminal.is_some() {
                    return Err(SemanticProblem::Duplicate);
                }
                if *outcome == EntryOutcome::Skipped && outline.target != PlannedTarget::Skipped {
                    return Err(SemanticProblem::Mismatch);
                }
                if *outcome == EntryOutcome::RetainedByReview
                    && (action != Action::Move
                        || outline.revision.map(|r| r.kind) != Some(Kind::Directory)
                        || s.directory_removal.is_some()
                        || (s.output.is_none()
                            && s.binding
                                .as_ref()
                                .and_then(|b| b.incumbent.as_ref())
                                .is_none())
                        || (s.lane == Some(OutputLane::Directory) && !s.directory_times))
                {
                    return Err(SemanticProblem::Mismatch);
                }
                if *outcome == EntryOutcome::Completed {
                    let kind = outline.revision.ok_or(SemanticProblem::Mismatch)?.kind;
                    let output_exists = s.output.is_some()
                        || s.binding
                            .as_ref()
                            .and_then(|b| b.incumbent.as_ref())
                            .is_some();
                    if !output_exists
                        || (kind == Kind::File && !s.verified)
                        || s.source_unsettled
                        || (kind == Kind::Directory
                            && s.lane == Some(OutputLane::Directory)
                            && !s.directory_times)
                        || (action == Action::Move
                            && kind == Kind::Directory
                            && !matches!(
                                s.directory_removal,
                                Some(Removal::Removed | Removal::Replaced(_))
                            ))
                        || (action == Action::Move
                            && s.lane == Some(OutputLane::Staged)
                            && (!s.published_verified
                                || s.source_state != Some(SourceState::Removed)))
                    {
                        return Err(SemanticProblem::Mismatch);
                    }
                }
                if *disposition == Disposition::NoEffect
                    && (s.stage.is_some()
                        || s.output.is_some()
                        || s.backup.is_some()
                        || s.pending.is_some())
                {
                    return Err(SemanticProblem::Mismatch);
                }
                if *disposition == Disposition::Settled
                    && (s.pending.is_some()
                        || s.stage.is_some()
                        || s.backup.is_some()
                        || s.source_unsettled)
                {
                    return Err(SemanticProblem::Mismatch);
                }
                s.terminal = Some((*outcome, *disposition));
            }
            Record::Finished { outcome } => {
                if self.states.iter().any(|s| s.terminal.is_none()) {
                    return Err(SemanticProblem::InvalidOrder);
                }
                let mut reviewed_children = vec![false; self.nodes.len()];
                let mut bad_children = vec![false; self.nodes.len()];
                for (index, node) in self.nodes.iter().enumerate() {
                    if let Some(parent) = node.parent {
                        let terminal = self.states[index]
                            .terminal
                            .ok_or(SemanticProblem::InvalidOrder)?
                            .0;
                        match terminal {
                            EntryOutcome::Skipped | EntryOutcome::RetainedByReview => {
                                reviewed_children[parent.0 as usize] = true
                            }
                            EntryOutcome::Completed => {}
                            _ => bad_children[parent.0 as usize] = true,
                        }
                    }
                }
                for (index, state) in self.states.iter().enumerate() {
                    let valid = match state.terminal {
                        Some((EntryOutcome::RetainedByReview, _)) => {
                            reviewed_children[index] && !bad_children[index]
                        }
                        Some((EntryOutcome::Completed, _)) if state.directory_removal.is_some() => {
                            !reviewed_children[index] && !bad_children[index]
                        }
                        _ => true,
                    };
                    if !valid {
                        return Err(SemanticProblem::Mismatch);
                    }
                }
                if *outcome == JobOutcome::Completed
                    && self.states.iter().any(|s| {
                        !matches!(
                            s.terminal,
                            Some((
                                EntryOutcome::Completed
                                    | EntryOutcome::Skipped
                                    | EntryOutcome::RetainedByReview,
                                _,
                            ))
                        ) || s.pending.is_some()
                            || s.source_unsettled
                    })
                {
                    return Err(SemanticProblem::Mismatch);
                }
                self.finished = true;
            }
        }
        Ok(())
    }
    fn effect(&mut self, id: NodeId, effect: &Effect) -> Result<(), SemanticProblem> {
        use Effect::*;
        let node = self
            .nodes
            .get(id.0 as usize)
            .ok_or(SemanticProblem::InvalidReference)?
            .clone();
        let mut state = self
            .states
            .get(id.0 as usize)
            .ok_or(SemanticProblem::InvalidReference)?
            .clone();
        if state.terminal.is_some() {
            return Err(SemanticProblem::InvalidOrder);
        }
        let binding = state.binding.clone().ok_or(SemanticProblem::InvalidOrder)?;
        let target = binding.target.clone();
        let source = node
            .source
            .clone()
            .ok_or(SemanticProblem::InvalidReference)?;
        let original_revision = node.revision.ok_or(SemanticProblem::InvalidReference)?;
        let require_source = |actual: &Object| {
            if *actual == source {
                Ok(())
            } else {
                Err(SemanticProblem::Mismatch)
            }
        };
        let require_pending = |actual: Option<Pending>, expected| {
            if actual == Some(expected) {
                Ok(())
            } else {
                Err(SemanticProblem::InvalidOrder)
            }
        };
        let require_output = |actual: &Object, state: &State| {
            if state.output.as_ref() == Some(actual) {
                Ok(())
            } else {
                Err(SemanticProblem::Mismatch)
            }
        };
        let mut update_parent = None;
        match effect {
            StageIntent { stage } => {
                self.place(stage)?;
                if stage.parent != target.parent
                    || stage.leaf.same_slot(&target.leaf)
                    || (stage.parent == source.place.parent
                        && stage.leaf.same_slot(&source.place.leaf))
                    || state.pending.is_some()
                    || state.stage.is_some()
                    || state.output.is_some()
                    || state.verified
                    || original_revision.kind != Kind::File
                {
                    return Err(SemanticProblem::InvalidOrder);
                }
                state.stage_intent = Some(stage.clone());
                state.pending = Some(Pending::Stage);
            }
            StageCreated { stage } => {
                self.object(stage)?;
                require_pending(state.pending, Pending::Stage)?;
                if state.stage_intent.as_ref() != Some(&stage.place) {
                    return Err(SemanticProblem::Mismatch);
                }
                state.stage = Some(stage.clone());
                state.pending = None;
            }
            DisplacementIntent { original, backup } => {
                self.object(original)?;
                self.place(backup)?;
                if state.pending.is_some()
                    || state.stage.is_none()
                    || binding.incumbent.as_ref() != Some(original)
                    || backup.parent != target.parent
                    || backup.leaf.same_slot(&target.leaf)
                    || backup.leaf.same_slot(&original.place.leaf)
                    || state.stage.as_ref().is_some_and(|stage| {
                        stage.place.parent == backup.parent
                            && stage.place.leaf.same_slot(&backup.leaf)
                    })
                {
                    return Err(SemanticProblem::Mismatch);
                }
                state.backup = Some((original.clone(), backup.clone()));
                state.pending = Some(Pending::Displace);
            }
            DisplacementObserved { original, backup } => {
                require_pending(state.pending, Pending::Displace)?;
                if state.backup.as_ref() != Some(&(original.clone(), backup.clone())) {
                    return Err(SemanticProblem::Mismatch);
                }
                state.pending = None;
            }
            PublishIntent {
                stage,
                target: dest,
            } => {
                self.object(stage)?;
                self.place(dest)?;
                if state.pending.is_some()
                    || state.stage.as_ref() != Some(stage)
                    || *dest != target
                    || (binding.incumbent.is_some() && state.backup.is_none())
                {
                    return Err(SemanticProblem::InvalidOrder);
                }
                state.pending = Some(Pending::Publish);
            }
            PublicationObserved { output } => {
                self.object(output)?;
                require_pending(state.pending, Pending::Publish)?;
                let stage = state.stage.as_ref().ok_or(SemanticProblem::InvalidOrder)?;
                if output.place != target || output.identity != stage.identity {
                    return Err(SemanticProblem::Mismatch);
                }
                state.output = Some(output.clone());
                state.lane = Some(OutputLane::Staged);
                state.stage = None;
                state.pending = None;
            }
            Verified {
                output,
                verification,
                bytes,
                revision,
            } => {
                self.object(output)?;
                require_output(output, &state)?;
                if state.pending.is_some()
                    || state.lane != Some(OutputLane::Staged)
                    || state.verified
                    || (self.header.map(|header| header.1) == Some(Action::Move)
                        && !matches!(verification, Verification::Verified { .. }))
                    || revision.kind != Kind::File
                    || revision.length != Some(*bytes)
                    || revision.length != original_revision.length
                    || revision.created != original_revision.created
                    || revision.modified != original_revision.modified
                {
                    return Err(SemanticProblem::Mismatch);
                }
                state.verified = true;
                state.published_verified = matches!(verification, Verification::Verified { .. });
            }
            RestoreIntent { original, backup } => {
                if state.backup.as_ref() != Some(&(original.clone(), backup.clone()))
                    || !matches!(state.pending, None | Some(Pending::Publish))
                    || state.output.is_some()
                {
                    return Err(SemanticProblem::InvalidOrder);
                }
                state.pending = Some(Pending::Restore);
            }
            RestoreObserved { original, backup } => {
                require_pending(state.pending, Pending::Restore)?;
                if state.backup.as_ref() != Some(&(original.clone(), backup.clone())) {
                    return Err(SemanticProblem::Mismatch);
                }
                state.backup = None;
                state.pending = None;
            }
            MoveRenameIntent {
                source: actual,
                target: dest,
                original,
            } => {
                self.object(actual)?;
                self.place(dest)?;
                require_source(actual)?;
                if *dest != target
                    || *original != original_revision
                    || state.pending.is_some()
                    || state.output.is_some()
                    || state.stage.is_some()
                    || state.verified
                    || self.header.map(|v| v.1) != Some(Action::Move)
                    || original_revision.kind != Kind::File
                    || binding.incumbent.is_some()
                    || source.identity.volume_serial_number()
                        != self.parent(target.parent)?.identity.volume_serial_number()
                {
                    return Err(SemanticProblem::Mismatch);
                }
                state.pending = Some(Pending::Move);
            }
            MoveRenameObserved {
                source: actual,
                target: dest,
                at_target,
                source_disposition,
            } => {
                require_source(actual)?;
                require_pending(state.pending, Pending::Move)?;
                if *dest != target {
                    return Err(SemanticProblem::Mismatch);
                }
                if let AtomicSourceDisposition::Foreign(id) = source_disposition {
                    if id.file_id() == [0; 16]
                        || *id == source.identity
                        || id.volume_serial_number() != source.identity.volume_serial_number()
                    {
                        return Err(SemanticProblem::Mismatch);
                    }
                }
                if *at_target == Some(true) {
                    state.output = Some(Object {
                        place: target.clone(),
                        identity: source.identity,
                    });
                    state.lane = Some(OutputLane::Atomic);
                    state.source_unsettled =
                        matches!(source_disposition, AtomicSourceDisposition::Unknown);
                    state.pending = None;
                } else if *at_target == Some(false) {
                    state.pending = None;
                }
            }
            MoveMetadataIntent { output } => {
                require_output(output, &state)?;
                if state.pending.is_some()
                    || state.lane != Some(OutputLane::Atomic)
                    || state.verified
                {
                    return Err(SemanticProblem::InvalidOrder);
                }
                state.pending = Some(Pending::MoveMetadata);
            }
            MoveMetadataObserved { output, revision } => {
                require_output(output, &state)?;
                require_pending(state.pending, Pending::MoveMetadata)?;
                if revision.kind != Kind::File
                    || revision.length != original_revision.length
                    || revision.created != original_revision.created
                    || revision.modified != original_revision.modified
                {
                    return Err(SemanticProblem::Mismatch);
                }
                state.pending = None;
            }
            MoveVerified {
                output, revision, ..
            } => {
                require_output(output, &state)?;
                if state.pending.is_some()
                    || state.lane != Some(OutputLane::Atomic)
                    || state.verified
                    || revision.kind != Kind::File
                    || revision.length != original_revision.length
                    || revision.created != original_revision.created
                    || revision.modified != original_revision.modified
                {
                    return Err(SemanticProblem::InvalidOrder);
                }
                state.verified = true;
            }
            StagedSourceIntent {
                action,
                source: actual,
                recovery,
                output,
                state: source_state,
            } => {
                require_source(actual)?;
                require_output(output, &state)?;
                if self.header.map(|v| v.1) != Some(Action::Move)
                    || !state.published_verified
                    || state.lane != Some(OutputLane::Staged)
                    || (state.pending.is_some()
                        && !(*action == SourceAction::Restore
                            && state.pending == Some(Pending::Staged(SourceAction::Dispose))
                            && state.source_state == Some(SourceState::Quarantined)))
                {
                    return Err(SemanticProblem::InvalidOrder);
                }
                let place = recovery.as_ref().ok_or(SemanticProblem::InvalidRecord)?;
                self.place(place)?;
                if place.parent != source.place.parent
                    || place.leaf.same_slot(&source.place.leaf)
                    || (place.parent == target.parent && place.leaf.same_slot(&target.leaf))
                    || state.backup.as_ref().is_some_and(|(_, backup)| {
                        backup.parent == place.parent && backup.leaf.same_slot(&place.leaf)
                    })
                {
                    return Err(SemanticProblem::Mismatch);
                }
                match action {
                    SourceAction::Relocate
                        if state.source_state.is_none()
                            && *source_state == SourceState::Original =>
                    {
                        state.recovery = Some(place.clone())
                    }
                    SourceAction::Dispose | SourceAction::Restore
                        if state.source_state == Some(SourceState::Quarantined)
                            && *source_state == SourceState::Quarantined
                            && state.recovery.as_ref() == Some(place) => {}
                    _ => return Err(SemanticProblem::InvalidOrder),
                }
                state.pending = Some(Pending::Staged(*action));
            }
            StagedSourceObserved {
                action,
                source: actual,
                recovery,
                output,
                state: observed,
            } => {
                require_source(actual)?;
                require_output(output, &state)?;
                require_pending(state.pending, Pending::Staged(*action))?;
                if recovery.as_ref() != state.recovery.as_ref() {
                    return Err(SemanticProblem::Mismatch);
                }
                match (action, observed) {
                    (SourceAction::Relocate, SourceState::Quarantined | SourceState::Uncertain)
                    | (
                        SourceAction::Dispose,
                        SourceState::Original
                        | SourceState::Quarantined
                        | SourceState::Removed
                        | SourceState::Uncertain,
                    )
                    | (
                        SourceAction::Restore,
                        SourceState::Original | SourceState::Quarantined | SourceState::Uncertain,
                    ) => {}
                    _ => return Err(SemanticProblem::InvalidOrder),
                }
                state.source_state = Some(*observed);
                state.source_unsettled =
                    matches!(observed, SourceState::Quarantined | SourceState::Uncertain);
                state.pending = None;
            }
            DirectoryCreateIntent { target: dest } => {
                self.place(dest)?;
                if original_revision.kind != Kind::Directory
                    || *dest != target
                    || state.pending.is_some()
                    || state.output.is_some()
                    || state.stage.is_some()
                    || state.verified
                    || binding.incumbent.is_some()
                {
                    return Err(SemanticProblem::Mismatch);
                }
                state.pending = Some(Pending::DirectoryCreate);
            }
            DirectoryCreated {
                output,
                parent_proof,
            } => {
                self.object(output)?;
                require_pending(state.pending, Pending::DirectoryCreate)?;
                let proof = self.parent(*parent_proof)?;
                if output.place != target
                    || proof.identity != output.identity
                    || proof.origin != ParentOrigin::Child(output.place.clone())
                {
                    return Err(SemanticProblem::Mismatch);
                }
                state.output = Some(output.clone());
                state.lane = Some(OutputLane::Directory);
                state.pending = None;
            }
            DirectoryMetadataIntent { output } => {
                self.object(output)?;
                require_output(output, &state)?;
                if state.pending.is_some()
                    || state.lane != Some(OutputLane::Directory)
                    || state.directory_times
                {
                    return Err(SemanticProblem::InvalidOrder);
                }
                state.pending = Some(Pending::DirectoryMetadata);
            }
            DirectoryMetadataObserved { output, revision } => {
                require_output(output, &state)?;
                require_pending(state.pending, Pending::DirectoryMetadata)?;
                if revision.kind != Kind::Directory {
                    return Err(SemanticProblem::Mismatch);
                }
                if revision.created != original_revision.created
                    || revision.modified != original_revision.modified
                {
                    return Err(SemanticProblem::Mismatch);
                }
                let key = (
                    Some(output.place.parent),
                    output.place.leaf.0.clone(),
                    output.identity,
                );
                let idx = self
                    .parent_keys
                    .get(&key)
                    .ok_or(SemanticProblem::InvalidReference)?
                    .0 as usize;
                update_parent = Some((idx, revision.created));
                state.directory_times = true;
                state.pending = None;
            }
            DirectoryRemoveIntent {
                source: actual,
                destination,
            } => {
                require_source(actual)?;
                if original_revision.kind != Kind::Directory
                    || state.pending.is_some()
                    || self.header.map(|v| v.1) != Some(Action::Move)
                {
                    return Err(SemanticProblem::InvalidOrder);
                }
                let proof = self.parent(*destination)?;
                let expected = state
                    .output
                    .as_ref()
                    .or(binding.incumbent.as_ref())
                    .ok_or(SemanticProblem::InvalidOrder)?;
                if proof.origin != ParentOrigin::Child(expected.place.clone())
                    || proof.identity != expected.identity
                {
                    return Err(SemanticProblem::Mismatch);
                }
                state.pending = Some(Pending::DirectoryRemove);
            }
            DirectoryRemoveObserved {
                source: actual,
                destination,
                observation,
            } => {
                require_source(actual)?;
                require_pending(state.pending, Pending::DirectoryRemove)?;
                let expected = state
                    .output
                    .as_ref()
                    .or(binding.incumbent.as_ref())
                    .ok_or(SemanticProblem::InvalidOrder)?;
                let proof = self.parent(*destination)?;
                if proof.origin != ParentOrigin::Child(expected.place.clone())
                    || proof.identity != expected.identity
                {
                    return Err(SemanticProblem::Mismatch);
                }
                if let Removal::Replaced(id) = observation {
                    if id.file_id() == [0; 16]
                        || *id == source.identity
                        || id.volume_serial_number() != source.identity.volume_serial_number()
                    {
                        return Err(SemanticProblem::Mismatch);
                    }
                }
                state.directory_removal = Some(*observation);
                state.source_unsettled = matches!(observation, Removal::Uncertain);
                state.pending = None;
            }
        }
        if let Some((index, created)) = update_parent {
            self.parents[index].created = created;
        }
        self.states[id.0 as usize] = state;
        Ok(())
    }
}

pub(super) fn encode(record: &Record) -> Result<Vec<u8>, SemanticProblem> {
    let bytes = serde_json::to_vec(record).map_err(|_| SemanticProblem::InvalidEncoding)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(SemanticProblem::Limit);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity(n: u8) -> NativeFileIdentity {
        NativeFileIdentity::from_parts(7, [n; 16])
    }
    fn name(s: &str) -> Name {
        Name(s.encode_utf16().collect())
    }
    fn place(parent: u32, s: &str) -> Place {
        Place {
            parent: ParentId(parent),
            leaf: name(s),
        }
    }
    fn revision(kind: Kind, created: i64) -> Revision {
        Revision {
            kind,
            length: if kind == Kind::File { Some(3) } else { None },
            attributes: 0,
            created,
            modified: 2,
            changed: 3,
        }
    }
    fn root() -> Record {
        Record::Parent(ParentProof {
            id: ParentId(0),
            origin: ParentOrigin::Root("C:\\".encode_utf16().collect()),
            identity: identity(1),
            created: 1,
        })
    }
    fn directory_node() -> Record {
        Record::Node(OutlineNode {
            id: NodeId(0),
            parent: None,
            source: Some(Object {
                place: place(0, "source"),
                identity: identity(2),
            }),
            revision: Some(revision(Kind::Directory, 5)),
            target: PlannedTarget::Direct {
                place: place(0, "dest"),
                policy: TargetPolicy::Vacant,
            },
        })
    }
    fn header() -> Record {
        Record::Header {
            id: ReceiptId([4; 16]),
            action: Action::Move,
            verification_requested: true,
        }
    }
    fn setup() -> SemanticPrefix {
        let mut prefix = SemanticPrefix::default();
        for r in [
            header(),
            root(),
            directory_node(),
            Record::Bind(Binding {
                node: NodeId(0),
                target: place(0, "dest"),
                incumbent: None,
                incumbent_revision: None,
            }),
        ] {
            assert!(prefix.push(r));
        }
        prefix
    }
    #[test]
    fn receipt_wire_prefix_stops_at_invalid_reference() {
        let mut prefix = setup();
        let before = prefix.records.len();
        let bad = Record::Effect {
            node: NodeId(18),
            effect: Effect::DirectoryCreateIntent {
                target: place(0, "dest"),
            },
        };
        assert!(!prefix.push_bytes(&encode(&bad).unwrap()));
        assert_eq!(prefix.records.len(), before);
        assert_eq!(prefix.problem, Some(SemanticProblem::InvalidReference));
        assert!(!prefix.push(Record::Finished {
            outcome: JobOutcome::Failed
        }));
    }
    #[test]
    fn receipt_wire_lineage_revision_updates_only_matching_created_identity() {
        let mut prefix = setup();
        let output = Object {
            place: place(0, "dest"),
            identity: identity(3),
        };
        assert!(prefix.push(Record::Effect {
            node: NodeId(0),
            effect: Effect::DirectoryCreateIntent {
                target: output.place.clone()
            }
        }));
        assert!(prefix.push(Record::Parent(ParentProof {
            id: ParentId(1),
            origin: ParentOrigin::Child(output.place.clone()),
            identity: output.identity,
            created: 10
        })));
        assert!(prefix.push(Record::Effect {
            node: NodeId(0),
            effect: Effect::DirectoryCreated {
                output: output.clone(),
                parent_proof: ParentId(1)
            }
        }));
        assert!(prefix.push(Record::Effect {
            node: NodeId(0),
            effect: Effect::DirectoryMetadataIntent {
                output: output.clone()
            }
        }));
        let mut wrong = output.clone();
        wrong.identity = identity(9);
        let before = prefix.records.len();
        assert!(!prefix.push(Record::Effect {
            node: NodeId(0),
            effect: Effect::DirectoryMetadataObserved {
                output: wrong,
                revision: revision(Kind::Directory, 5)
            }
        }));
        assert_eq!(prefix.parents[1].created, 10);
        assert_eq!(prefix.records.len(), before);
        let mut prefix = setup();
        for record in [
            Record::Effect {
                node: NodeId(0),
                effect: Effect::DirectoryCreateIntent {
                    target: output.place.clone(),
                },
            },
            Record::Parent(ParentProof {
                id: ParentId(1),
                origin: ParentOrigin::Child(output.place.clone()),
                identity: output.identity,
                created: 10,
            }),
            Record::Effect {
                node: NodeId(0),
                effect: Effect::DirectoryCreated {
                    output: output.clone(),
                    parent_proof: ParentId(1),
                },
            },
            Record::Effect {
                node: NodeId(0),
                effect: Effect::DirectoryMetadataIntent {
                    output: output.clone(),
                },
            },
            Record::Effect {
                node: NodeId(0),
                effect: Effect::DirectoryMetadataObserved {
                    output,
                    revision: revision(Kind::Directory, 5),
                },
            },
        ] {
            assert!(prefix.push(record));
        }
        assert_eq!(prefix.parents[1].created, 5);
    }
    #[test]
    fn receipt_wire_rejects_second_stage_after_verified_output() {
        let mut prefix = SemanticPrefix::default();
        assert!(prefix.push(header()));
        assert!(prefix.push(root()));
        let source = Object {
            place: place(0, "file"),
            identity: identity(2),
        };
        let target = place(0, "copy");
        assert!(prefix.push(Record::Node(OutlineNode {
            id: NodeId(0),
            parent: None,
            source: Some(source),
            revision: Some(revision(Kind::File, 5)),
            target: PlannedTarget::Direct {
                place: target.clone(),
                policy: TargetPolicy::Vacant
            }
        })));
        assert!(prefix.push(Record::Bind(Binding {
            node: NodeId(0),
            target: target.clone(),
            incumbent: None,
            incumbent_revision: None
        })));
        let stage = Object {
            place: place(0, "stage"),
            identity: identity(3),
        };
        let output = Object {
            place: target,
            identity: identity(3),
        };
        for effect in [
            Effect::StageIntent {
                stage: stage.place.clone(),
            },
            Effect::StageCreated {
                stage: stage.clone(),
            },
            Effect::PublishIntent {
                stage,
                target: output.place.clone(),
            },
            Effect::PublicationObserved {
                output: output.clone(),
            },
            Effect::Verified {
                output,
                verification: Verification::Verified {
                    sha256: [1; 32],
                    zone_sha256: None,
                },
                bytes: 3,
                revision: revision(Kind::File, 5),
            },
        ] {
            assert!(prefix.push(Record::Effect {
                node: NodeId(0),
                effect
            }));
        }
        assert!(!prefix.push(Record::Effect {
            node: NodeId(0),
            effect: Effect::StageIntent {
                stage: place(0, "stage2")
            }
        }));
    }
    #[test]
    fn receipt_wire_restore_supersedes_pending_dispose() {
        let mut prefix = SemanticPrefix::default();
        assert!(prefix.push(header()));
        assert!(prefix.push(root()));
        let source = Object {
            place: place(0, "file"),
            identity: identity(2),
        };
        let target = place(0, "copy");
        assert!(prefix.push(Record::Node(OutlineNode {
            id: NodeId(0),
            parent: None,
            source: Some(source.clone()),
            revision: Some(revision(Kind::File, 5)),
            target: PlannedTarget::Direct {
                place: target.clone(),
                policy: TargetPolicy::Vacant
            }
        })));
        assert!(prefix.push(Record::Bind(Binding {
            node: NodeId(0),
            target: target.clone(),
            incumbent: None,
            incumbent_revision: None
        })));
        let stage = Object {
            place: place(0, "stage"),
            identity: identity(3),
        };
        let output = Object {
            place: target,
            identity: identity(3),
        };
        for effect in [
            Effect::StageIntent {
                stage: stage.place.clone(),
            },
            Effect::StageCreated {
                stage: stage.clone(),
            },
            Effect::PublishIntent {
                stage,
                target: output.place.clone(),
            },
            Effect::PublicationObserved {
                output: output.clone(),
            },
            Effect::Verified {
                output: output.clone(),
                verification: Verification::Verified {
                    sha256: [1; 32],
                    zone_sha256: None,
                },
                bytes: 3,
                revision: revision(Kind::File, 5),
            },
        ] {
            assert!(prefix.push(Record::Effect {
                node: NodeId(0),
                effect
            }));
        }
        let recovery = place(0, "recovery");
        let make = |action, state, observed| {
            if observed {
                Effect::StagedSourceObserved {
                    action,
                    source: source.clone(),
                    recovery: Some(recovery.clone()),
                    state,
                    output: output.clone(),
                }
            } else {
                Effect::StagedSourceIntent {
                    action,
                    source: source.clone(),
                    recovery: Some(recovery.clone()),
                    state,
                    output: output.clone(),
                }
            }
        };
        for effect in [
            make(SourceAction::Relocate, SourceState::Original, false),
            make(SourceAction::Relocate, SourceState::Quarantined, true),
            make(SourceAction::Dispose, SourceState::Quarantined, false),
            make(SourceAction::Restore, SourceState::Quarantined, false),
            make(SourceAction::Restore, SourceState::Original, true),
        ] {
            assert!(prefix.push(Record::Effect {
                node: NodeId(0),
                effect
            }));
        }
    }
    #[test]
    fn receipt_wire_skipped_unavailable_and_unsettled_intent() {
        let mut prefix = SemanticPrefix::default();
        assert!(prefix.push(header()));
        assert!(prefix.push(root()));
        assert!(prefix.push(Record::Node(OutlineNode {
            id: NodeId(0),
            parent: None,
            source: None,
            revision: None,
            target: PlannedTarget::Skipped
        })));
        assert!(prefix.push(Record::EntryTerminal {
            node: NodeId(0),
            outcome: EntryOutcome::Skipped,
            disposition: Disposition::NoEffect
        }));
        assert!(prefix.push(Record::Finished {
            outcome: JobOutcome::Partial
        }));
        assert!(prefix.settled());
        let mut prefix = setup();
        assert!(prefix.push(Record::Effect {
            node: NodeId(0),
            effect: Effect::DirectoryCreateIntent {
                target: place(0, "dest")
            }
        }));
        assert!(prefix.push(Record::EntryTerminal {
            node: NodeId(0),
            outcome: EntryOutcome::Failed,
            disposition: Disposition::Uncertain
        }));
        assert!(prefix.push(Record::Finished {
            outcome: JobOutcome::Failed
        }));
        assert!(!prefix.settled());
    }
}
