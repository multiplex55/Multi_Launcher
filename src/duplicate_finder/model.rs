use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiscoveryStatus {
    Idle,
    Scanning,
    Complete,
    Partial,
    Cancelled,
    Failed,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RootScanStatus {
    Pending,
    Scanning,
    Complete,
    Partial,
    InvalidRoot,
    Inaccessible,
    UnsupportedIdentity,
    ChangedDuringScan,
    ResourceLimit,
    Cancelled,
    SkippedOverlap,
    SkippedEquivalent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScanCounts {
    pub(crate) directories_scanned: usize,
    pub(crate) entries_seen: usize,
    pub(crate) files_seen: usize,
    pub(crate) directories_seen: usize,
    pub(crate) other_entries_seen: usize,
    pub(crate) reparse_entries_excluded: usize,
    pub(crate) cloud_placeholders_excluded: usize,
    pub(crate) inaccessible_entries: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RootScan {
    pub(crate) selected_path: PathBuf,
    pub(crate) canonical_root: Option<PathBuf>,
    pub(crate) identity: Option<KnownFileIdentity>,
    pub(crate) status: RootScanStatus,
    pub(crate) counts: ScanCounts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiscoveryProgress {
    pub(crate) generation: u64,
    pub(crate) roots_total: usize,
    pub(crate) roots_preflighted: usize,
    pub(crate) roots_completed: usize,
    pub(crate) current_root: Option<PathBuf>,
    pub(crate) counts: ScanCounts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiscoveryDiagnostic {
    pub(crate) root_index: Option<usize>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) message: String,
}

/// A reliable identity from a validated native handle. All-zero IDs are
/// deliberately represented by `None` at the scanner boundary, never by a
/// fabricated per-path identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct KnownFileIdentity {
    pub(crate) volume_serial_number: u64,
    pub(crate) file_id: [u8; 16],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileAlias {
    pub(crate) root_index: usize,
    pub(crate) path: PathBuf,
}

/// One distinct native file object, regardless of the number of hard-link
/// paths through which it was encountered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KnownFile {
    pub(crate) identity: KnownFileIdentity,
    pub(crate) size_bytes: u64,
    pub(crate) modified: Option<SystemTime>,
    pub(crate) aliases: Vec<FileAlias>,
}

/// A path observation with an unsupported identity. It is kept separate for
/// later explicitly labeled M11-B content matching and is not a distinct-file
/// count or verified copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdentityUnverifiedPath {
    pub(crate) root_index: usize,
    pub(crate) size_bytes: u64,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SizeGroup {
    pub(crate) size_bytes: u64,
    /// Each item is one known file identity; aliases do not increase this
    /// vector's length.
    pub(crate) known_files: Vec<KnownFile>,
    /// These paths are observations only. They are not counted as independent
    /// files until content and identity verification is available.
    pub(crate) unverified_paths: Vec<IdentityUnverifiedPath>,
}

#[derive(Debug)]
pub(crate) struct DiscoverySnapshot {
    pub(crate) generation: u64,
    pub(crate) status: DiscoveryStatus,
    pub(crate) progress: Option<DiscoveryProgress>,
    pub(crate) roots: Vec<RootScan>,
    pub(crate) size_groups: Vec<SizeGroup>,
    pub(crate) diagnostics: Vec<DiscoveryDiagnostic>,
    pub(crate) diagnostics_truncated: bool,
    pub(crate) counts: ScanCounts,
}

impl DiscoverySnapshot {
    pub(crate) fn idle(generation: u64) -> Self {
        Self {
            generation,
            status: DiscoveryStatus::Idle,
            progress: None,
            roots: Vec::new(),
            size_groups: Vec::new(),
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            counts: ScanCounts::default(),
        }
    }

    pub(crate) fn scanning(generation: u64, roots: Vec<PathBuf>) -> Self {
        Self {
            generation,
            status: DiscoveryStatus::Scanning,
            progress: Some(DiscoveryProgress {
                generation,
                roots_total: roots.len(),
                roots_preflighted: 0,
                roots_completed: 0,
                current_root: None,
                counts: ScanCounts::default(),
            }),
            roots: roots
                .into_iter()
                .map(|selected_path| RootScan {
                    selected_path,
                    canonical_root: None,
                    identity: None,
                    status: RootScanStatus::Pending,
                    counts: ScanCounts::default(),
                })
                .collect(),
            size_groups: Vec::new(),
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            counts: ScanCounts::default(),
        }
    }
}
