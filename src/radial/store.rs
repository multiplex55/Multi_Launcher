use super::model::{
    CURRENT_SCHEMA_VERSION, ConfigRevision, MenuId, RADIAL_FILE, RadialDocument, SkinId,
};
use super::validation::{ValidationErrors, validate};
use crate::common::persistence::{LoadState, PersistenceError, read_bytes, save_json_atomic};
use crate::platform::app_data::AppDataRoot;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateState {
    Missing,
    Empty,
}

#[derive(Debug)]
pub enum StoreError {
    Candidate(CandidateState),
    Persistence(PersistenceError),
    Malformed {
        path: PathBuf,
        source: serde_json::Error,
    },
    UnsupportedNewerVersion {
        found: u64,
        supported: u32,
    },
    Validation(ValidationErrors),
    RevisionConflict {
        expected: ConfigRevision,
        actual: ConfigRevision,
    },
    RevisionOverflow {
        revision: ConfigRevision,
    },
    LockPoisoned,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Candidate(state) => write!(f, "radial store is {state:?}"),
            Self::Persistence(error) => error.fmt(f),
            Self::Malformed { path, source } => write!(
                f,
                "malformed radial document at {}: {source}",
                path.display()
            ),
            Self::UnsupportedNewerVersion { found, supported } => write!(
                f,
                "radial schema {found} is newer than supported schema {supported}"
            ),
            Self::Validation(error) => error.fmt(f),
            Self::RevisionConflict { expected, actual } => write!(
                f,
                "radial revision conflict: expected {}, current {}",
                expected.0, actual.0
            ),
            Self::RevisionOverflow { revision } => {
                write!(f, "radial revision {} cannot be incremented", revision.0)
            }
            Self::LockPoisoned => f.write_str("radial snapshot lock is poisoned"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<PersistenceError> for StoreError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}
impl From<ValidationErrors> for StoreError {
    fn from(value: ValidationErrors) -> Self {
        Self::Validation(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceImpact {
    pub paths: Vec<String>,
}

pub struct RadialStore {
    path: PathBuf,
    published: RwLock<Arc<RadialDocument>>,
    transaction: Mutex<()>,
}

impl RadialStore {
    pub fn new(root: &AppDataRoot) -> Result<Self, StoreError> {
        Self::at_path(root.path().join(RADIAL_FILE), RadialDocument::starter())
    }
    pub fn at_path(path: impl Into<PathBuf>, initial: RadialDocument) -> Result<Self, StoreError> {
        validate(&initial)?;
        Ok(Self {
            path: path.into(),
            published: RwLock::new(Arc::new(initial)),
            transaction: Mutex::new(()),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn snapshot(&self) -> Result<Arc<RadialDocument>, StoreError> {
        self.published
            .read()
            .map(|v| Arc::clone(&v))
            .map_err(|_| StoreError::LockPoisoned)
    }

    /// Load and publish only a fully parsed and validated candidate.
    pub fn reload(&self) -> Result<Arc<RadialDocument>, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let bytes = match read_bytes(&self.path)? {
            LoadState::Missing => return Err(StoreError::Candidate(CandidateState::Missing)),
            LoadState::Empty => return Err(StoreError::Candidate(CandidateState::Empty)),
            LoadState::Loaded(bytes) => bytes,
        };
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|source| StoreError::Malformed {
                path: self.path.clone(),
                source,
            })?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if version > CURRENT_SCHEMA_VERSION as u64 {
            return Err(StoreError::UnsupportedNewerVersion {
                found: version,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }
        let candidate: RadialDocument =
            serde_json::from_value(value).map_err(|source| StoreError::Malformed {
                path: self.path.clone(),
                source,
            })?;
        validate(&candidate)?;
        let candidate = Arc::new(candidate);
        *self
            .published
            .write()
            .map_err(|_| StoreError::LockPoisoned)? = Arc::clone(&candidate);
        Ok(candidate)
    }

    /// Validate, atomically persist, then publish a single monotonic revision.
    pub fn save(
        &self,
        expected: ConfigRevision,
        mut candidate: RadialDocument,
    ) -> Result<Arc<RadialDocument>, StoreError> {
        let _transaction = self
            .transaction
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let actual = self.snapshot()?.revision;
        if actual != expected {
            return Err(StoreError::RevisionConflict { expected, actual });
        }
        candidate.schema_version = CURRENT_SCHEMA_VERSION;
        candidate.revision = ConfigRevision(
            actual
                .0
                .checked_add(1)
                .ok_or(StoreError::RevisionOverflow { revision: actual })?,
        );
        validate(&candidate)?;
        save_json_atomic(&self.path, &candidate)?;
        let candidate = Arc::new(candidate);
        *self
            .published
            .write()
            .map_err(|_| StoreError::LockPoisoned)? = Arc::clone(&candidate);
        Ok(candidate)
    }
}

pub fn references_to_menu(document: &RadialDocument, menu_id: &MenuId) -> ReferenceImpact {
    let mut paths = Vec::new();
    if &document.default_menu_id == menu_id {
        paths.push("default_menu_id".into());
    }
    for (mi, menu) in document.menus.iter().enumerate() {
        for (ri, ring) in menu.rings.iter().enumerate() {
            for (ci, cell) in ring.cells.iter().enumerate() {
                if matches!(&cell.content, super::model::CellContent::Submenu { menu_id: target } if target == menu_id)
                {
                    paths.push(format!("menus[{mi}].rings[{ri}].cells[{ci}].content"));
                }
            }
        }
    }
    for (index, rule) in document.context_rules.iter().enumerate() {
        if &rule.menu_id == menu_id {
            paths.push(format!("context_rules[{index}].menu_id"));
        }
    }
    for (index, trigger) in document.custom_triggers.iter().enumerate() {
        if &trigger.menu_id == menu_id {
            paths.push(format!("custom_triggers[{index}].menu_id"));
        }
    }
    ReferenceImpact { paths }
}
pub fn references_to_skin(document: &RadialDocument, skin_id: &SkinId) -> ReferenceImpact {
    ReferenceImpact {
        paths: document
            .menus
            .iter()
            .enumerate()
            .filter(|(_, menu)| &menu.skin_id == skin_id)
            .map(|(index, _)| format!("menus[{index}].skin_id"))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn fixture() -> (tempfile::TempDir, RadialStore) {
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(dir.path());
        (dir, RadialStore::new(&root).unwrap())
    }
    #[test]
    fn missing_empty_malformed_and_newer_are_distinct_and_retain_snapshot() {
        let (dir, store) = fixture();
        let before = store.snapshot().unwrap();
        assert!(matches!(
            store.reload(),
            Err(StoreError::Candidate(CandidateState::Missing))
        ));
        fs::write(dir.path().join(RADIAL_FILE), "  ").unwrap();
        assert!(matches!(
            store.reload(),
            Err(StoreError::Candidate(CandidateState::Empty))
        ));
        fs::write(dir.path().join(RADIAL_FILE), "{").unwrap();
        assert!(matches!(store.reload(), Err(StoreError::Malformed { .. })));
        fs::write(dir.path().join(RADIAL_FILE), r#"{"schema_version":999}"#).unwrap();
        assert!(matches!(
            store.reload(),
            Err(StoreError::UnsupportedNewerVersion { .. })
        ));
        assert!(Arc::ptr_eq(&before, &store.snapshot().unwrap()));
    }
    #[test]
    fn save_is_revision_checked_atomic_and_publishes_after_success() {
        let (_dir, store) = fixture();
        let before = store.snapshot().unwrap();
        let saved = store.save(before.revision, (*before).clone()).unwrap();
        assert_eq!(saved.revision.0, 2);
        assert_eq!(store.reload().unwrap().revision, saved.revision);
        assert!(matches!(
            store.save(ConfigRevision(1), (*saved).clone()),
            Err(StoreError::RevisionConflict { .. })
        ));
    }
    #[test]
    fn invalid_save_preserves_file_and_last_valid_snapshot() {
        let (dir, store) = fixture();
        let initial = store.snapshot().unwrap();
        let saved = store.save(initial.revision, (*initial).clone()).unwrap();
        let bytes = fs::read(dir.path().join(RADIAL_FILE)).unwrap();
        let mut bad = (*saved).clone();
        bad.default_menu_id = MenuId::new("missing");
        assert!(matches!(
            store.save(saved.revision, bad),
            Err(StoreError::Validation(_))
        ));
        assert_eq!(fs::read(dir.path().join(RADIAL_FILE)).unwrap(), bytes);
        assert_eq!(store.snapshot().unwrap().revision, saved.revision);
    }
    #[test]
    fn impacts_are_explicit_before_deletion() {
        let d = RadialDocument::starter();
        assert_eq!(
            references_to_menu(&d, &d.default_menu_id).paths,
            vec!["default_menu_id"]
        );
        assert_eq!(
            references_to_skin(&d, &d.skins[0].id).paths,
            vec!["menus[0].skin_id"]
        );
    }

    #[test]
    fn at_path_rejects_an_invalid_initial_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let mut invalid = RadialDocument::starter();
        invalid.default_menu_id = MenuId::new("missing");
        assert!(matches!(
            RadialStore::at_path(directory.path().join(RADIAL_FILE), invalid),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn revision_overflow_is_reported_without_writing_or_publishing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(RADIAL_FILE);
        let mut initial = RadialDocument::starter();
        initial.revision = ConfigRevision(u64::MAX);
        let store = RadialStore::at_path(&path, initial.clone()).unwrap();
        assert!(matches!(
            store.save(initial.revision, initial.clone()),
            Err(StoreError::RevisionOverflow {
                revision: ConfigRevision(u64::MAX)
            })
        ));
        assert!(!path.exists());
        assert_eq!(store.snapshot().unwrap().revision, ConfigRevision(u64::MAX));
    }
}
