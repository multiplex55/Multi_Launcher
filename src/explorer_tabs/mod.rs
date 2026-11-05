//! Bounded, evidence-based Explorer folder-tab operations.
//!
//! The operation model is platform independent. The Windows adapter only
//! observes Shell state; opening or navigating tabs is intentionally owned by
//! a later integration layer.

mod identity;
mod operation;
#[cfg(windows)]
mod windows;

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
