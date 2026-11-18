//! Mutation records required by guarded transfer effects.
use crate::file_operations::NativeFileIdentity;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(super) enum TransferJournalEvent {
    DirectoryCreateIntent {
        entry: super::SourceNodeId,
        source: super::EntrySnapshot,
        path: PathBuf,
        parent: NativeFileIdentity,
    },
    DirectoryCreated {
        entry: super::SourceNodeId,
        source: super::EntrySnapshot,
        output: super::CopyArtifact,
        destination: super::ParentSnapshot,
    },
    DirectoryMetadataIntent {
        entry: super::SourceNodeId,
        source: super::EntrySnapshot,
        output: super::CopyArtifact,
    },
    DirectoryMetadataObserved {
        entry: super::SourceNodeId,
        output: super::CopyArtifact,
        revision: super::EntryMetadata,
    },
    DirectoryRemoveIntent {
        entry: super::SourceNodeId,
        source: super::EntrySnapshot,
        destination: super::ParentSnapshot,
    },
    DirectoryRemoveObserved {
        entry: super::SourceNodeId,
        source: super::EntrySnapshot,
        destination: super::ParentSnapshot,
        observation: super::execute::directory_move::DirectoryRemovalObservation,
    },
    StagedSourceIntent {
        action: super::copy::StagedSourceAction,
        effect: super::StagedMoveEffect,
    },
    StagedSourceObserved {
        action: super::copy::StagedSourceAction,
        effect: super::StagedMoveEffect,
    },
    MoveRenameIntent {
        source: PathBuf,
        target: PathBuf,
        source_parent: NativeFileIdentity,
        target_parent: NativeFileIdentity,
        identity: NativeFileIdentity,
        original: super::EntryMetadata,
    },
    MoveRenameObserved {
        effect: super::relocate::MoveEffect,
    },
    MoveMetadataIntent {
        effect: super::relocate::MoveEffect,
    },
    MoveMetadataObserved {
        effect: super::relocate::MoveEffect,
    },
    MoveVerified {
        effect: super::relocate::MoveEffect,
    },
    StageIntent {
        path: PathBuf,
        parent: NativeFileIdentity,
    },
    StageCreated {
        path: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
    },
    DisplacementIntent {
        original: PathBuf,
        backup: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
    },
    DisplacementObserved {
        original: PathBuf,
        backup: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
    },
    PublishIntent {
        stage: PathBuf,
        target: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
    },
    PublicationObserved {
        target: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
    },
    Verified {
        target: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
        verification: super::CopyVerification,
        bytes: u64,
        revision: super::EntryMetadata,
    },
    RestoreIntent {
        original: PathBuf,
        backup: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
    },
    RestoreObserved {
        original: PathBuf,
        backup: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
    },
}

pub(super) trait TransferJournal {
    fn begin_operation(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn end_operation(&mut self) {}
    fn begin_entry(
        &mut self,
        _id: super::SourceNodeId,
        _target: &super::TargetSnapshot,
    ) -> io::Result<()> {
        Ok(())
    }
    fn select_entry(&mut self, _id: super::SourceNodeId) -> io::Result<()> {
        Ok(())
    }
    fn record(&mut self, event: TransferJournalEvent) -> io::Result<()>;
}
