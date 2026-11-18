//! Mutation records required by guarded transfer effects.
use crate::file_operations::NativeFileIdentity;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(super) enum TransferJournalEvent {
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
    fn record(&mut self, event: TransferJournalEvent) -> io::Result<()>;
}
