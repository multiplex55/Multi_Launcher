//! Bounded, read-only duplicate-file discovery and content verification.
//!
//! One scan worker owns directory traversal, bounded streaming hashes, and
//! exact byte comparisons. Reliable file identities and identity-unverified
//! path matches remain separate in the immutable result snapshot.

mod content;
mod discovery;
mod model;
mod session;
pub(crate) mod projection;
mod api;

pub use api::{DuplicateFinder, DuplicateFinderCounts, DuplicateFinderProgress, DuplicateFinderReport, DuplicateFinderRow,
    DuplicateFinderStatus, DuplicateFinderTotals, DuplicateFinderView, MAX_ROOTS, MAX_ROOT_PATH_BYTES};
pub use discovery::path_bytes as encoded_root_path_bytes;
pub use session::DuplicateFinderSessionError;

#[cfg(any(test, feature = "dev-smoke-binaries"))]
pub(crate) use content::FirstReadCheckpointGate;

pub(crate) use model::{
    ContentProgress, DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStage,
    DiscoveryStatus, DuplicateTotals, FileAlias, IdentityUnverifiedMatch, IdentityUnverifiedPath,
    KnownFile, KnownFileIdentity, RootScan, RootScanStatus, ScanCounts, SizeGroup,
    VerifiedDuplicateGroup, VerifiedKnownFile,
};
pub(crate) use session::{DuplicateFinderSession, DuplicateFinderSessionView};

#[cfg(test)]
mod tests;
