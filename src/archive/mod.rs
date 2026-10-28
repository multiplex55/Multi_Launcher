//! Bounded ZIP metadata cataloging and pure extraction-plan construction.
//!
//! This module deliberately stops before reading entry payloads or touching an
//! extraction destination. It produces identities and lexical paths for a later
//! owner that must independently enforce source and destination safety.

mod catalog;
mod model;
mod plan;

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
