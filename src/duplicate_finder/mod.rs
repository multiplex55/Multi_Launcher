//! Bounded, read-only duplicate-file discovery.
//!
//! This checkpoint groups discovered files by size and reliable file
//! identity. Content hashing and equality verification are intentionally
//! owned by the later duplicate-finder engine.

mod discovery;
mod model;
mod session;

pub(crate) use model::{
    DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStatus, FileAlias,
    IdentityUnverifiedPath, KnownFile, KnownFileIdentity, RootScan, RootScanStatus, ScanCounts,
    SizeGroup,
};
pub(crate) use session::{
    DuplicateFinderSession, DuplicateFinderSessionError, DuplicateFinderSessionView,
};

#[cfg(test)]
mod tests;
