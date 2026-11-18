//! Projection of approved plans and engine evidence into receipt records.
use super::wire::*;
use std::os::windows::ffi::OsStrExt;
/// Converts admitted in-memory proofs to data-only records. The caller persists
/// returned records in order before allowing the corresponding engine effect.
pub(super) struct ReceiptProjection {
    pub prefix: SemanticPrefix,
    cursor: usize,
    active: Option<NodeId>,
    projected_bytes: u64,
}
impl ReceiptProjection {
    pub fn new(
        id: ReceiptId,
        plan: &crate::file_transfer::ApprovedTransferPlan,
    ) -> Result<Self, SemanticProblem> {
        use crate::file_transfer::{ApprovedTarget, SourceObservation, TransferAction};
        let action = match plan.draft().action {
            TransferAction::CopyTo(_) => Action::Copy,
            TransferAction::MoveTo(_) => Action::Move,
            TransferAction::Duplicate => Action::Duplicate,
        };
        let mut this = Self {
            prefix: SemanticPrefix::default(),
            cursor: 0,
            active: None,
            projected_bytes: 12,
        };
        this.add(Record::Header {
            id,
            action,
            verification_requested: plan.draft().verify,
        })?;
        if let Some(origin) = plan.recovery_origin() {
            this.add(Record::RecoveryOrigin {
                id: ReceiptId(origin.receipt.0),
                node: origin.node,
                verb: match origin.verb {
                    crate::file_transfer::plan::RecoveryVerb::Retry => RecoveryVerb::Retry,
                    crate::file_transfer::plan::RecoveryVerb::Undo => RecoveryVerb::Undo,
                },
            })?;
        }
        let mut approved = std::collections::HashMap::new();
        for entry in plan.entries() {
            approved.insert(entry.id().index(), entry);
        }
        for row in plan.source().entries() {
            let id = NodeId(u32::try_from(row.id().index()).map_err(|_| SemanticProblem::Limit)?);
            let (source, revision) = match row.observation() {
                SourceObservation::Ready(snapshot) => (
                    Some(this.snapshot_object(snapshot)?),
                    Some(snapshot.metadata().into()),
                ),
                SourceObservation::Unavailable(_) => (None, None),
            };
            let target = match approved.get(&row.id().index()).map(|e| e.destination()) {
                None => PlannedTarget::Skipped,
                Some(ApprovedTarget::CreateAt(t)) => PlannedTarget::Direct {
                    place: this.target_place(t)?,
                    policy: TargetPolicy::Vacant,
                },
                Some(ApprovedTarget::Replace(t)) => {
                    let crate::file_transfer::TargetOccupancy::Occupied(incumbent) = t.occupancy()
                    else {
                        return Err(SemanticProblem::Mismatch);
                    };
                    PlannedTarget::Direct {
                        place: this.target_place(t)?,
                        policy: TargetPolicy::Replace {
                            incumbent: this.snapshot_object(incumbent)?,
                            revision: incumbent.metadata().into(),
                        },
                    }
                }
                Some(ApprovedTarget::Merge { target, .. }) => {
                    let crate::file_transfer::TargetOccupancy::Occupied(incumbent) =
                        target.occupancy()
                    else {
                        return Err(SemanticProblem::Mismatch);
                    };
                    PlannedTarget::Direct {
                        place: this.target_place(target)?,
                        policy: TargetPolicy::Merge {
                            incumbent: this.snapshot_object(incumbent)?,
                            revision: incumbent.metadata().into(),
                        },
                    }
                }
                Some(ApprovedTarget::CreateIn { parent, leaf }) => PlannedTarget::Child {
                    parent: NodeId(
                        u32::try_from(parent.index()).map_err(|_| SemanticProblem::Limit)?,
                    ),
                    leaf: Name(leaf.as_os_str().encode_wide().collect()),
                },
            };
            this.add(Record::Node(OutlineNode {
                id,
                parent: row.parent().map(|v| NodeId(v.index() as u32)),
                source,
                revision,
                target,
            }))?;
        }
        Ok(this)
    }
    pub(super) fn add(&mut self, record: Record) -> Result<(), SemanticProblem> {
        let encoded = super::wire::encode(&record)?;
        let size =
            super::frames::FrameWriter::frame_size(encoded.len()).ok_or(SemanticProblem::Limit)?;
        let total = self
            .projected_bytes
            .checked_add(size)
            .ok_or(SemanticProblem::Limit)?;
        if total > super::frames::MAX_RECEIPT_BYTES {
            return Err(SemanticProblem::Limit);
        }
        if self.prefix.push(record) {
            self.projected_bytes = total;
            Ok(())
        } else {
            Err(self
                .prefix
                .problem
                .unwrap_or(SemanticProblem::InvalidRecord))
        }
    }
    pub fn take_records(&mut self) -> Vec<Record> {
        let out = self.prefix.records[self.cursor..].to_vec();
        self.cursor = self.prefix.records.len();
        out
    }
    fn intern_parent(
        &mut self,
        snapshot: &crate::file_transfer::ParentSnapshot,
    ) -> Result<ParentId, SemanticProblem> {
        use std::os::windows::ffi::OsStrExt;
        let (root, parts) =
            crate::archive::transfer_native::split_local_absolute_path(snapshot.path())
                .map_err(|_| SemanticProblem::InvalidRecord)?;
        if snapshot.chain.len() != parts.len() + 1 {
            return Err(SemanticProblem::Mismatch);
        }
        let mut parent = None;
        for (i, stamp) in snapshot.chain.iter().enumerate() {
            let (origin, key_leaf) = if i == 0 {
                let root: Vec<_> = root.as_os_str().encode_wide().collect();
                (ParentOrigin::Root(root.clone()), root)
            } else {
                let leaf = Name(parts[i - 1].encode_wide().collect());
                (
                    ParentOrigin::Child(Place {
                        parent: parent.ok_or(SemanticProblem::InvalidReference)?,
                        leaf: leaf.clone(),
                    }),
                    leaf.0,
                )
            };
            let key = (parent, key_leaf, stamp.identity);
            let id = if let Some(id) = self.prefix.parent_keys.get(&key).copied() {
                if self.prefix.parent(id)?.created != stamp.created {
                    return Err(SemanticProblem::Mismatch);
                }
                id
            } else {
                let id = ParentId(
                    u32::try_from(self.prefix.parents.len()).map_err(|_| SemanticProblem::Limit)?,
                );
                self.add(Record::Parent(ParentProof {
                    id,
                    origin,
                    identity: stamp.identity,
                    created: stamp.created,
                }))?;
                id
            };
            parent = Some(id);
        }
        parent.ok_or(SemanticProblem::InvalidReference)
    }
    fn snapshot_object(
        &mut self,
        snapshot: &crate::file_transfer::EntrySnapshot,
    ) -> Result<Object, SemanticProblem> {
        use std::os::windows::ffi::OsStrExt;
        Ok(Object {
            place: Place {
                parent: self.intern_parent(snapshot.parent())?,
                leaf: Name(snapshot.leaf().as_os_str().encode_wide().collect()),
            },
            identity: snapshot.identity(),
        })
    }
    fn target_place(
        &mut self,
        target: &crate::file_transfer::TargetSnapshot,
    ) -> Result<Place, SemanticProblem> {
        use std::os::windows::ffi::OsStrExt;
        Ok(Place {
            parent: self.intern_parent(target.parent())?,
            leaf: Name(target.requested_leaf().as_os_str().encode_wide().collect()),
        })
    }
    pub fn begin_entry(
        &mut self,
        id: crate::file_transfer::SourceNodeId,
        target: &crate::file_transfer::TargetSnapshot,
    ) -> Result<(), SemanticProblem> {
        use crate::file_transfer::TargetOccupancy;
        let node = NodeId(u32::try_from(id.index()).map_err(|_| SemanticProblem::Limit)?);
        let place = self.target_place(target)?;
        let incumbent = match target.occupancy() {
            TargetOccupancy::Vacant => None,
            TargetOccupancy::Occupied(value) => Some(self.snapshot_object(value)?),
        };
        self.add(Record::Bind(Binding {
            node,
            target: place,
            incumbent,
            incumbent_revision: match target.occupancy() {
                TargetOccupancy::Vacant => None,
                TargetOccupancy::Occupied(value) => Some(value.metadata().into()),
            },
        }))?;
        self.active = Some(node);
        Ok(())
    }
    pub fn select_entry(
        &mut self,
        id: crate::file_transfer::SourceNodeId,
    ) -> Result<(), SemanticProblem> {
        let node = NodeId(u32::try_from(id.index()).map_err(|_| SemanticProblem::Limit)?);
        if self
            .prefix
            .states
            .get(node.0 as usize)
            .and_then(|s| s.binding.as_ref())
            .is_none()
        {
            return Err(SemanticProblem::InvalidReference);
        }
        self.active = Some(node);
        Ok(())
    }
}
impl From<crate::file_transfer::EntryMetadata> for Revision {
    fn from(value: crate::file_transfer::EntryMetadata) -> Self {
        Self {
            kind: value.kind.into(),
            length: value.byte_len,
            attributes: value.attributes,
            created: value.created,
            modified: value.modified,
            changed: value.changed,
        }
    }
}
impl From<crate::file_transfer::CopyVerification> for Verification {
    fn from(value: crate::file_transfer::CopyVerification) -> Self {
        match value {
            crate::file_transfer::CopyVerification::NotRequested => Self::NotRequested,
            crate::file_transfer::CopyVerification::Verified {
                sha256,
                zone_sha256,
            } => Self::Verified {
                sha256,
                zone_sha256,
            },
        }
    }
}
impl ReceiptProjection {
    fn active(&self) -> Result<(NodeId, &OutlineNode, &Binding), SemanticProblem> {
        let id = self.active.ok_or(SemanticProblem::InvalidOrder)?;
        let node = self
            .prefix
            .nodes
            .get(id.0 as usize)
            .ok_or(SemanticProblem::InvalidReference)?;
        let binding = self
            .prefix
            .states
            .get(id.0 as usize)
            .and_then(|s| s.binding.as_ref())
            .ok_or(SemanticProblem::InvalidOrder)?;
        Ok((id, node, binding))
    }
    fn location(
        &self,
        path: &std::path::Path,
        parent_identity: crate::file_operations::NativeFileIdentity,
    ) -> Result<Place, SemanticProblem> {
        let (_, node, binding) = self.active()?;
        let source = node
            .source
            .as_ref()
            .ok_or(SemanticProblem::InvalidReference)?;
        let (root, parts) = crate::archive::transfer_native::split_local_absolute_path(path)
            .map_err(|_| SemanticProblem::InvalidRecord)?;
        let (leaf, parent_parts) = parts.split_last().ok_or(SemanticProblem::InvalidRecord)?;
        let mut parent_path: Vec<u16> = root.as_os_str().encode_wide().collect();
        for part in parent_parts {
            if parent_path.last() != Some(&(b'\\' as u16)) {
                parent_path.push(b'\\' as u16);
            }
            parent_path.extend(part.encode_wide());
        }
        for parent in [source.place.parent, binding.target.parent] {
            if self.prefix.parent(parent)?.identity == parent_identity
                && self.prefix.parent_path(parent)? == parent_path
            {
                let result = Place {
                    parent,
                    leaf: Name(leaf.encode_wide().collect()),
                };
                if !result.leaf.valid() {
                    return Err(SemanticProblem::InvalidRecord);
                }
                return Ok(result);
            }
        }
        Err(SemanticProblem::Mismatch)
    }
    fn object_at(
        &self,
        path: &std::path::Path,
        parent: crate::file_operations::NativeFileIdentity,
        identity: crate::file_operations::NativeFileIdentity,
    ) -> Result<Object, SemanticProblem> {
        Ok(Object {
            place: self.location(path, parent)?,
            identity,
        })
    }
    /// Converts one engine journal event for the selected entry. Parent proof
    /// records, when needed, precede the returned effect record.
    pub fn project_event(
        &mut self,
        event: &crate::file_transfer::journal::TransferJournalEvent,
    ) -> Result<(), SemanticProblem> {
        use crate::file_transfer::copy::StagedSourceAction;
        use crate::file_transfer::journal::TransferJournalEvent as E;
        use crate::file_transfer::{
            DirectoryRemovalObservation, MoveLocation, StagedMoveSourceState,
        };
        let (node, outline, binding) = self.active()?;
        let source = outline
            .source
            .clone()
            .ok_or(SemanticProblem::InvalidReference)?;
        let target = binding.target.clone();
        let mut intern = None;
        let effect = match event {
            E::StageIntent { path, parent } => Effect::StageIntent {
                stage: self.location(path, *parent)?,
            },
            E::StageCreated {
                path,
                parent,
                identity,
            } => Effect::StageCreated {
                stage: self.object_at(path, *parent, *identity)?,
            },
            E::DisplacementIntent {
                original,
                backup,
                parent,
                identity,
            } => Effect::DisplacementIntent {
                original: self.object_at(original, *parent, *identity)?,
                backup: self.location(backup, *parent)?,
            },
            E::DisplacementObserved {
                original,
                backup,
                parent,
                identity,
            } => Effect::DisplacementObserved {
                original: self.object_at(original, *parent, *identity)?,
                backup: self.location(backup, *parent)?,
            },
            E::PublishIntent {
                stage,
                target,
                parent,
                identity,
            } => Effect::PublishIntent {
                stage: self.object_at(stage, *parent, *identity)?,
                target: self.location(target, *parent)?,
            },
            E::PublicationObserved {
                target,
                parent,
                identity,
            } => Effect::PublicationObserved {
                output: self.object_at(target, *parent, *identity)?,
            },
            E::Verified {
                target,
                parent,
                identity,
                verification,
                bytes,
                revision,
            } => Effect::Verified {
                output: self.object_at(target, *parent, *identity)?,
                verification: (*verification).into(),
                bytes: *bytes,
                revision: (*revision).into(),
            },
            E::RestoreIntent {
                original,
                backup,
                parent,
                identity,
            } => Effect::RestoreIntent {
                original: self.object_at(original, *parent, *identity)?,
                backup: self.location(backup, *parent)?,
            },
            E::RestoreObserved {
                original,
                backup,
                parent,
                identity,
            } => Effect::RestoreObserved {
                original: self.object_at(original, *parent, *identity)?,
                backup: self.location(backup, *parent)?,
            },
            E::MoveRenameIntent {
                source: path,
                target: dest,
                source_parent,
                target_parent,
                identity,
                original,
            } => Effect::MoveRenameIntent {
                source: self.object_at(path, *source_parent, *identity)?,
                target: self.location(dest, *target_parent)?,
                original: (*original).into(),
            },
            E::MoveRenameObserved { effect } => Effect::MoveRenameObserved {
                source: self.object_at(&effect.source, effect.source_parent, effect.identity)?,
                target: self.location(&effect.destination, effect.destination_parent)?,
                source_disposition: match effect.source_disposition {
                    crate::file_transfer::MoveSourceDisposition::Vacant => {
                        AtomicSourceDisposition::Vacant
                    }
                    crate::file_transfer::MoveSourceDisposition::Foreign(id) => {
                        AtomicSourceDisposition::Foreign(id)
                    }
                    crate::file_transfer::MoveSourceDisposition::Unknown => {
                        AtomicSourceDisposition::Unknown
                    }
                },
                at_target: match effect.location {
                    MoveLocation::Source => Some(false),
                    MoveLocation::Destination => Some(true),
                    MoveLocation::Uncertain => None,
                },
            },
            E::MoveMetadataIntent { effect } => Effect::MoveMetadataIntent {
                output: self.object_at(
                    &effect.destination,
                    effect.destination_parent,
                    effect.identity,
                )?,
            },
            E::MoveMetadataObserved { effect } => Effect::MoveMetadataObserved {
                output: self.object_at(
                    &effect.destination,
                    effect.destination_parent,
                    effect.identity,
                )?,
                revision: effect
                    .final_metadata
                    .ok_or(SemanticProblem::Mismatch)?
                    .into(),
            },
            E::MoveVerified { effect } => Effect::MoveVerified {
                output: self.object_at(
                    &effect.destination,
                    effect.destination_parent,
                    effect.identity,
                )?,
                revision: effect
                    .final_metadata
                    .ok_or(SemanticProblem::Mismatch)?
                    .into(),
                verification: effect.verification.ok_or(SemanticProblem::Mismatch)?.into(),
            },
            E::StagedSourceIntent { action, effect }
            | E::StagedSourceObserved { action, effect } => {
                let action = match action {
                    StagedSourceAction::Relocate => SourceAction::Relocate,
                    StagedSourceAction::Dispose => SourceAction::Dispose,
                    StagedSourceAction::Restore => SourceAction::Restore,
                };
                let state = match effect.source_state {
                    StagedMoveSourceState::Original => SourceState::Original,
                    StagedMoveSourceState::Quarantined => SourceState::Quarantined,
                    StagedMoveSourceState::Removed => SourceState::Removed,
                    StagedMoveSourceState::Uncertain => SourceState::Uncertain,
                };
                let actual =
                    self.object_at(&effect.source, effect.source_parent, effect.source_identity)?;
                let recovery = effect
                    .recovery
                    .as_ref()
                    .map(|p| self.location(p, effect.source_parent))
                    .transpose()?;
                let output = self.object_at(
                    &effect.output.path,
                    effect.output.parent,
                    effect.output.identity,
                )?;
                if matches!(event, E::StagedSourceIntent { .. }) {
                    Effect::StagedSourceIntent {
                        action,
                        source: actual,
                        recovery,
                        state,
                        output,
                    }
                } else {
                    Effect::StagedSourceObserved {
                        action,
                        source: actual,
                        recovery,
                        state,
                        output,
                    }
                }
            }
            E::DirectoryCreateIntent {
                entry,
                source: actual,
                path,
                parent,
            } => {
                if entry.index() != node.0 as usize
                    || self.snapshot_object_readonly(actual)? != source
                {
                    return Err(SemanticProblem::Mismatch);
                }
                Effect::DirectoryCreateIntent {
                    target: self.location(path, *parent)?,
                }
            }
            E::DirectoryCreated {
                entry,
                source: actual,
                output,
                destination,
            } => {
                if entry.index() != node.0 as usize
                    || self.snapshot_object_readonly(actual)? != source
                {
                    return Err(SemanticProblem::Mismatch);
                }
                let object = self.object_at(&output.path, output.parent, output.identity)?;
                intern = Some(destination.clone());
                Effect::DirectoryCreated {
                    output: object,
                    parent_proof: ParentId(u32::MAX),
                }
            }
            E::DirectoryMetadataIntent {
                entry,
                source: actual,
                output,
            } => {
                if entry.index() != node.0 as usize
                    || self.snapshot_object_readonly(actual)? != source
                {
                    return Err(SemanticProblem::Mismatch);
                }
                Effect::DirectoryMetadataIntent {
                    output: self.object_at(&output.path, output.parent, output.identity)?,
                }
            }
            E::DirectoryMetadataObserved {
                entry,
                output,
                revision,
            } => {
                if entry.index() != node.0 as usize {
                    return Err(SemanticProblem::Mismatch);
                }
                Effect::DirectoryMetadataObserved {
                    output: self.object_at(&output.path, output.parent, output.identity)?,
                    revision: (*revision).into(),
                }
            }
            E::DirectoryRemoveIntent {
                entry,
                source: actual,
                destination,
            } => {
                if entry.index() != node.0 as usize
                    || self.snapshot_object_readonly(actual)? != source
                {
                    return Err(SemanticProblem::Mismatch);
                }
                intern = Some(destination.clone());
                Effect::DirectoryRemoveIntent {
                    source: source.clone(),
                    destination: ParentId(u32::MAX),
                }
            }
            E::DirectoryRemoveObserved {
                entry,
                source: actual,
                destination,
                observation,
            } => {
                if entry.index() != node.0 as usize
                    || self.snapshot_object_readonly(actual)? != source
                {
                    return Err(SemanticProblem::Mismatch);
                }
                intern = Some(destination.clone());
                let observation = match observation {
                    DirectoryRemovalObservation::Removed => Removal::Removed,
                    DirectoryRemovalObservation::Replaced(id) => Removal::Replaced(*id),
                    DirectoryRemovalObservation::Retained => Removal::Retained,
                    DirectoryRemovalObservation::Uncertain => Removal::Uncertain,
                };
                Effect::DirectoryRemoveObserved {
                    source: source.clone(),
                    destination: ParentId(u32::MAX),
                    observation,
                }
            }
        };
        let _ = target;
        let effect = if let Some(snapshot) = intern {
            let id = self.intern_parent(&snapshot)?;
            match effect {
                Effect::DirectoryCreated { output, .. } => Effect::DirectoryCreated {
                    output,
                    parent_proof: id,
                },
                Effect::DirectoryRemoveIntent { source, .. } => Effect::DirectoryRemoveIntent {
                    source,
                    destination: id,
                },
                Effect::DirectoryRemoveObserved {
                    source,
                    observation,
                    ..
                } => Effect::DirectoryRemoveObserved {
                    source,
                    destination: id,
                    observation,
                },
                _ => return Err(SemanticProblem::InvalidRecord),
            }
        } else {
            effect
        };
        self.add(Record::Effect { node, effect })
    }
    fn snapshot_object_readonly(
        &self,
        value: &crate::file_transfer::EntrySnapshot,
    ) -> Result<Object, SemanticProblem> {
        self.object_at(&value.path(), value.parent().identity(), value.identity())
    }
}
