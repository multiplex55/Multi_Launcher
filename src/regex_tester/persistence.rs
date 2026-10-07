//! Domain-owned local storage using the shared atomic JSON boundary.

use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use super::model::RegexFlags;

mod presets;
pub use presets::{PresetContent, PresetId, PresetInput, PresetStore, RegexPreset};

pub const MAX_RECENT_REGEXES: usize = 50;
const HISTORY_VERSION: u32 = 1;

/// History never contains test text or replacement buffers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEntry {
    pub pattern: String,
    pub flags: RegexFlags,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryDocument {
    version: u32,
    entries: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreLoadStatus {
    Missing,
    Empty,
    Loaded,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDiagnostic {
    pub message: String,
}

#[derive(Debug)]
pub enum RegexStoreError {
    Persistence(PersistenceError),
    UnsupportedVersion { path: PathBuf, version: u32 },
    ReloadRequired { path: PathBuf },
    Validation { path: PathBuf, message: String },
}

impl fmt::Display for RegexStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persistence(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedVersion { path, version } => write!(
                formatter,
                "unsupported regex storage version {version} in {}",
                path.display()
            ),
            Self::ReloadRequired { path } => write!(
                formatter,
                "regex storage at {} requires a successful reload before changes can be saved",
                path.display()
            ),
            Self::Validation { path, message } => {
                write!(
                    formatter,
                    "invalid regex storage data for {}: {message}",
                    path.display()
                )
            }
        }
    }
}

impl Error for RegexStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Persistence(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PersistenceError> for RegexStoreError {
    fn from(error: PersistenceError) -> Self {
        Self::Persistence(error)
    }
}

/// Shared read-failure latch and recoverable diagnostics for domain stores.
struct StoreHealth {
    load_status: StoreLoadStatus,
    diagnostic: Option<StoreDiagnostic>,
}

impl StoreHealth {
    fn new() -> Self {
        Self {
            load_status: StoreLoadStatus::Missing,
            diagnostic: None,
        }
    }

    fn is_writable(&self) -> bool {
        self.load_status != StoreLoadStatus::Blocked
    }

    fn ensure_writable(&self, path: &Path) -> Result<(), RegexStoreError> {
        if self.is_writable() {
            Ok(())
        } else {
            Err(RegexStoreError::ReloadRequired {
                path: path.to_owned(),
            })
        }
    }

    fn publish(&mut self, status: StoreLoadStatus) {
        self.load_status = status;
        self.diagnostic = None;
    }

    fn report(&mut self, error: &impl fmt::Display) {
        self.diagnostic = Some(StoreDiagnostic {
            message: error.to_string(),
        });
    }

    fn block(&mut self, error: &RegexStoreError) {
        self.load_status = StoreLoadStatus::Blocked;
        self.report(error);
    }
}

/// Newest-first recent expressions. Failed reads latch write protection until
/// an explicit successful reload; a reload failure preserves the last snapshot.
pub struct HistoryStore {
    path: PathBuf,
    entries: Vec<HistoryEntry>,
    health: StoreHealth,
}

impl HistoryStore {
    /// Loads without creating or rewriting the file. Errors remain available
    /// through `diagnostic`, with mutations blocked until successful reload.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let mut store = Self {
            path: path.into(),
            entries: Vec::new(),
            health: StoreHealth::new(),
        };
        let _ = store.reload();
        store
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn load_status(&self) -> StoreLoadStatus {
        self.health.load_status
    }

    pub fn diagnostic(&self) -> Option<&StoreDiagnostic> {
        self.health.diagnostic.as_ref()
    }

    /// Whether the last read permits mutation. The next record still checks
    /// disk and may fail; this is not a filesystem permission guarantee.
    pub fn is_writable(&self) -> bool {
        self.health.is_writable()
    }

    pub fn reload(&mut self) -> Result<(), RegexStoreError> {
        match read_history(&self.path) {
            Ok((load_status, entries)) => {
                self.entries = entries;
                self.health.publish(load_status);
                Ok(())
            }
            Err(error) => {
                self.health.block(&error);
                Err(error)
            }
        }
    }

    /// Promotes an exact pattern/flags pair. Reads the current disk snapshot
    /// before mutation, then publishes memory only after atomic save succeeds.
    pub fn record(&mut self, pattern: &str, flags: RegexFlags) -> Result<(), RegexStoreError> {
        self.health.ensure_writable(&self.path)?;
        let (_, mut candidate) = match read_history(&self.path) {
            Ok(loaded) => loaded,
            Err(error) => {
                self.health.block(&error);
                return Err(error);
            }
        };
        candidate.retain(|entry| entry.pattern != pattern || entry.flags != flags);
        candidate.insert(
            0,
            HistoryEntry {
                pattern: pattern.to_owned(),
                flags,
            },
        );
        candidate.truncate(MAX_RECENT_REGEXES);
        let document = HistoryDocument {
            version: HISTORY_VERSION,
            entries: candidate,
        };
        if let Err(error) = save_json_atomic(&self.path, &document) {
            self.health.report(&error);
            return Err(error.into());
        }
        self.entries = document.entries;
        self.health.publish(StoreLoadStatus::Loaded);
        Ok(())
    }
}

fn read_history(path: &Path) -> Result<(StoreLoadStatus, Vec<HistoryEntry>), RegexStoreError> {
    match load_json::<HistoryDocument>(path)? {
        LoadState::Missing => Ok((StoreLoadStatus::Missing, Vec::new())),
        LoadState::Empty => Ok((StoreLoadStatus::Empty, Vec::new())),
        LoadState::Loaded(document) => {
            if document.version != HISTORY_VERSION {
                return Err(RegexStoreError::UnsupportedVersion {
                    path: path.to_owned(),
                    version: document.version,
                });
            }
            let mut entries = Vec::with_capacity(document.entries.len().min(MAX_RECENT_REGEXES));
            for entry in document.entries {
                if !entries.contains(&entry) {
                    entries.push(entry);
                    if entries.len() == MAX_RECENT_REGEXES {
                        break;
                    }
                }
            }
            Ok((StoreLoadStatus::Loaded, entries))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_document(path: &Path, entries: Vec<HistoryEntry>) {
        save_json_atomic(
            path,
            &HistoryDocument {
                version: HISTORY_VERSION,
                entries,
            },
        )
        .unwrap();
    }

    fn entry(pattern: &str) -> HistoryEntry {
        HistoryEntry {
            pattern: pattern.to_owned(),
            flags: RegexFlags::default(),
        }
    }

    #[test]
    fn missing_and_empty_files_are_distinct_writable_and_not_rewritten_on_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let missing = HistoryStore::open(&path);
        assert_eq!(missing.load_status(), StoreLoadStatus::Missing);
        assert!(missing.is_writable());
        assert!(!path.exists());
        std::fs::write(&path, b" \t\r\n").unwrap();
        let mut empty = HistoryStore::open(&path);
        assert_eq!(empty.load_status(), StoreLoadStatus::Empty);
        assert_eq!(std::fs::read(&path).unwrap(), b" \t\r\n");
        empty.record("a", RegexFlags::default()).unwrap();
        assert_eq!(HistoryStore::open(&path).entries(), &[entry("a")]);
    }

    #[test]
    fn records_deduplicate_promote_and_keep_distinct_flag_combinations() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let flags = RegexFlags::default();
        let mut store = HistoryStore::open(&path);
        store.record("a", flags).unwrap();
        store.record("b", flags).unwrap();
        store.record("a", flags).unwrap();
        let insensitive = RegexFlags {
            case_insensitive: true,
            ..flags
        };
        store.record("a", insensitive).unwrap();
        assert_eq!(
            store.entries(),
            &[
                HistoryEntry {
                    pattern: "a".into(),
                    flags: insensitive
                },
                entry("a"),
                entry("b")
            ]
        );
        assert_eq!(HistoryStore::open(&path).entries(), store.entries());
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted["version"], 1);
        for entry in persisted["entries"].as_array().unwrap() {
            assert_eq!(
                entry
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                ["flags", "pattern"]
            );
        }
    }

    #[test]
    fn loading_normalizes_duplicates_and_cap_without_rewriting_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let mut entries = vec![entry("newest"), entry("newest")];
        entries.extend((0..MAX_RECENT_REGEXES + 5).map(|index| entry(&format!("pattern-{index}"))));
        write_document(&path, entries);
        let original = std::fs::read(&path).unwrap();
        let mut store = HistoryStore::open(&path);
        assert_eq!(store.entries().len(), MAX_RECENT_REGEXES);
        assert_eq!(store.entries()[0], entry("newest"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        store.record("promoted", RegexFlags::default()).unwrap();
        assert_eq!(store.entries().len(), MAX_RECENT_REGEXES);
        assert_eq!(store.entries()[0], entry("promoted"));
        assert_eq!(HistoryStore::open(&path).entries(), store.entries());
    }

    #[test]
    fn malformed_schema_or_version_preserves_original_until_explicit_successful_reload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        for bytes in [
            b"{broken".as_slice(),
            br#"{"version":1,"entries":"wrong"}"#,
            br#"{"version":2,"entries":[]}"#,
            br#"{"version":1,"entries":[{"pattern":"a","flags":{},"test_text":"private"}]}"#,
            br#"{"version":1,"entries":[{"pattern":"a","flags":{"case_insensitve":true}}]}"#,
            br#"{"version":1,"entries":[{"pattern":"a","flags":{"unicode":true,"future_flag":true}}]}"#,
        ] {
            std::fs::write(&path, bytes).unwrap();
            let mut store = HistoryStore::open(&path);
            assert!(!store.is_writable());
            assert!(store.diagnostic().is_some());
            assert!(store.record("new", RegexFlags::default()).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            write_document(&path, vec![entry("repaired")]);
            assert!(matches!(
                store.record("new", RegexFlags::default()),
                Err(RegexStoreError::ReloadRequired { .. })
            ));
            store.reload().unwrap();
            store.record("new", RegexFlags::default()).unwrap();
            assert_eq!(store.entries(), &[entry("new"), entry("repaired")]);
            assert!(store.diagnostic().is_none());
        }
    }

    #[test]
    fn external_corruption_blocks_mutation_and_failed_reload_retains_last_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let mut store = HistoryStore::open(&path);
        store.record("valid", RegexFlags::default()).unwrap();
        std::fs::write(&path, b"broken external edit").unwrap();
        assert!(store.record("new", RegexFlags::default()).is_err());
        assert_eq!(store.entries(), &[entry("valid")]);
        assert!(store.reload().is_err());
        assert_eq!(store.entries(), &[entry("valid")]);
        assert_eq!(std::fs::read(&path).unwrap(), b"broken external edit");
        write_document(&path, vec![entry("external")]);
        store.reload().unwrap();
        store.record("new", RegexFlags::default()).unwrap();
        assert_eq!(store.entries(), &[entry("new"), entry("external")]);
    }

    #[test]
    fn mutation_uses_current_valid_disk_state_instead_of_stale_memory() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let mut store = HistoryStore::open(&path);
        store.record("old", RegexFlags::default()).unwrap();
        write_document(&path, vec![entry("external")]);
        store.record("new", RegexFlags::default()).unwrap();
        assert_eq!(store.entries(), &[entry("new"), entry("external")]);
    }

    #[test]
    fn read_failure_is_blocked_and_preserves_existing_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let mut store = HistoryStore::open(&path);
        store.record("valid", RegexFlags::default()).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(store.reload().is_err());
        assert!(!store.is_writable());
        assert_eq!(store.entries(), &[entry("valid")]);
        assert!(store.record("new", RegexFlags::default()).is_err());
        assert!(path.is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn atomic_write_failure_does_not_publish_memory_or_replace_disk() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let mut store = HistoryStore::open(&path);
        store.record("valid", RegexFlags::default()).unwrap();
        let original = std::fs::read(&path).unwrap();
        // Allow read/write sharing but deny delete sharing, preventing atomic
        // replacement while permitting the store's pre-mutation disk check.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        assert!(matches!(
            store.record("new", RegexFlags::default()),
            Err(RegexStoreError::Persistence(
                PersistenceError::AtomicWrite { .. }
            ))
        ));
        assert_eq!(store.entries(), &[entry("valid")]);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(store.diagnostic().is_some());
        drop(lock);
        store.record("new", RegexFlags::default()).unwrap();
        assert_eq!(store.entries(), &[entry("new"), entry("valid")]);
        assert!(store.diagnostic().is_none());
    }
}
