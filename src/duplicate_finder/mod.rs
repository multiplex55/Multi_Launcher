//! Bounded, read-only duplicate-file discovery and content verification.
//!
//! One scan worker owns directory traversal, bounded streaming hashes, and
//! exact byte comparisons. Reliable file identities and identity-unverified
//! path matches remain separate in the immutable result snapshot.

mod content;
mod discovery;
mod model;
mod session;

#[cfg(any(test, feature = "dev-smoke-binaries"))]
pub(crate) use content::FirstReadCheckpointGate;

pub(crate) use model::{
    ContentProgress, DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStage,
    DiscoveryStatus, DuplicateTotals, FileAlias, IdentityUnverifiedMatch, IdentityUnverifiedPath,
    KnownFile, KnownFileIdentity, RootScan, RootScanStatus, ScanCounts, SizeGroup,
    VerifiedDuplicateGroup, VerifiedKnownFile,
};
pub(crate) use session::{
    DuplicateFinderSession, DuplicateFinderSessionError, DuplicateFinderSessionView,
};

#[cfg(test)]
mod tests;
