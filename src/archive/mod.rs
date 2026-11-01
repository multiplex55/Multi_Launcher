//! Bounded ZIP metadata cataloging and pure extraction-plan construction.
//!
//! The portable catalog and plan remain metadata-only. On Windows, the
//! private native writer separately validates held source and destination
//! authority before streaming supported Stored entries.

mod catalog;
pub(crate) mod crc32;
mod model;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod native_io;
mod plan;
#[cfg(windows)]
mod session;
#[cfg(windows)]
mod writer;

pub use catalog::read_catalog;
pub use model::{
    ArchiveCatalog, ArchiveEntry, ArchiveEntryId, ArchiveEntryName, ArchiveFormat,
    ArchiveFormatProblem, ArchiveLimits, ArchiveRevision, ArchiveSource, CatalogError,
    CatalogLimit, EntryCompression, EntryKind, EntryProblems, NameEncodingProblem,
    SafeRelativePath, UnsafeEntryReason, UnsafePathReason, ValidatedRelativePath,
};
pub use plan::{
    ArchiveSelection, CapturedOutputPolicy, DestinationOccupancy, ExtractionPlan, PlanBuildError,
    PlanIssue, PlanIssueReason, PlanLimit, PlanReport, PlannedOutput, SelectionAddError,
    SelectionTarget, build_extraction_plan,
};

#[cfg(windows)]
pub(crate) use session::{
    ArchivePreviewSummary, ArchiveSession, ArchiveSessionFailure, ArchiveSessionPhase,
    ArchiveSessionRequestError, ArchiveSessionSnapshot, PreviewToken,
};
