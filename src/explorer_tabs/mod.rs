//! Bounded, evidence-based Explorer folder-tab operations.
//!
//! The operation model and guarded worker driver are platform independent.

mod backend;
mod evidence;
mod folder_lists;
mod identity;
#[cfg(windows)]
mod native_uia;
mod operation;
#[cfg(windows)]
mod windows;

pub use backend::{
    BackendDiagnostic, BootstrapState, ExplorerTabsJob, ExplorerTabsJobState, ExplorerTabsReport,
    ExplorerTabsStartError,
};
pub use folder_lists::{
    CURRENT_FOLDER_LISTS_VERSION, FOLDER_LIST_FILE_NAME, FolderListError, FolderListId,
    FolderListRunSnapshot, FolderListStore, MAX_FOLDER_LIST_NAME_BYTES, MAX_FOLDER_LISTS_BYTES,
    MAX_SAVED_FOLDER_LISTS, SavedFolderList,
};
pub use identity::{
    CandidateCorrelation, PidlPathMatch, ShellBindingIdentity, ShellPathComparison,
    ShellTargetCandidate, ShellViewCandidate, ShellViewIdentity, ShellViewSnapshot,
    SnapshotFailure, WindowIdentity, correlate_shell_candidate,
};
pub use operation::{
    BoundTargetEvidence, FolderIdentityEvidence, FolderIdentityResult, FolderPath, FolderPathError,
    FolderRequestError, FolderRequestKind, FolderRequestStep, FolderTabOperation, FolderTabRequest,
    InitialTargetEvidence, OperationAction, OperationControl, OperationFailure, OperationId,
    OperationOutcome, OperationReport, RequestResult, TargetAuthorityId, VerifiedFolderTab,
    VerifiedTabEvidence, VerifiedTabIdentity,
};

#[cfg(windows)]
pub use windows::{ShellObservationError, WindowsShellObserver};
