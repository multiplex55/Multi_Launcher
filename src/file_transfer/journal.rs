//! Mutation records required by the guarded replacement path.
use crate::file_operations::NativeFileIdentity;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(super) enum TransferJournalEvent {
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
