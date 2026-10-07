use super::{RegexStoreError, StoreDiagnostic, StoreHealth, StoreLoadStatus};
use crate::common::persistence::{LoadState, load_json, save_json_atomic};
use crate::regex_tester::model::RegexFlags;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

const PRESET_VERSION: u32 = 1;

/// Stable nonzero identifier. IDs are never reused after deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PresetId(NonZeroU64);

impl PresetId {
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn value(self) -> u64 {
        self.0.get()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegexPreset {
    pub id: PresetId,
    pub name: String,
    pub pattern: String,
    pub flags: RegexFlags,
    pub sample_text: Option<String>,
    pub replacement: Option<String>,
}

/// Explicit save input, preserving absent values separately from empty text.
/// Incomplete patterns are permitted so users can save work in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetInput {
    pub name: String,
    pub pattern: String,
    pub flags: RegexFlags,
    pub sample_text: Option<String>,
    pub replacement: Option<String>,
}

impl PresetInput {
    fn into_preset(self, id: PresetId, name: String) -> RegexPreset {
        RegexPreset {
            id,
            name,
            pattern: self.pattern,
            flags: self.flags,
            sample_text: self.sample_text,
            replacement: self.replacement,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetDocument {
    version: u32,
    next_id: u64,
    presets: Vec<RegexPreset>,
}

impl Default for PresetDocument {
    fn default() -> Self {
        Self {
            version: PRESET_VERSION,
            next_id: 1,
            presets: Vec::new(),
        }
    }
}

pub struct PresetStore {
    path: PathBuf,
    document: PresetDocument,
    health: StoreHealth,
}

impl PresetStore {
    /// Loads without creating a file; failed loads remain recoverable diagnostics.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let mut store = Self {
            path: path.into(),
            document: PresetDocument::default(),
            health: StoreHealth::new(),
        };
        let _ = store.reload();
        store
    }

    pub fn entries(&self) -> &[RegexPreset] {
        &self.document.presets
    }
    pub fn get(&self, id: PresetId) -> Option<&RegexPreset> {
        self.entries().iter().find(|preset| preset.id == id)
    }
    pub fn load_status(&self) -> StoreLoadStatus {
        self.health.load_status
    }
    pub fn diagnostic(&self) -> Option<&StoreDiagnostic> {
        self.health.diagnostic.as_ref()
    }
    pub fn is_writable(&self) -> bool {
        self.health.is_writable()
    }

    pub fn reload(&mut self) -> Result<(), RegexStoreError> {
        match read_presets(&self.path) {
            Ok((status, document)) => {
                self.document = document;
                self.health.publish(status);
                Ok(())
            }
            Err(error) => {
                self.health.block(&error);
                Err(error)
            }
        }
    }

    pub fn create(&mut self, input: PresetInput) -> Result<PresetId, RegexStoreError> {
        self.mutate(move |document, path| {
            let name = validate_name(path, &document.presets, &input.name, None)?;
            let id = PresetId::new(document.next_id)
                .ok_or_else(|| invalid(path, "next_id must be nonzero"))?;
            let next_id = document
                .next_id
                .checked_add(1)
                .ok_or_else(|| invalid(path, "preset identifier space is exhausted"))?;
            document.presets.push(input.into_preset(id, name));
            document.next_id = next_id;
            Ok(id)
        })
    }

    pub fn rename(&mut self, id: PresetId, name: &str) -> Result<(), RegexStoreError> {
        self.mutate(|document, path| {
            let index = preset_index(path, document, id)?;
            let name = validate_name(path, &document.presets, name, Some(id))?;
            document.presets[index].name = name;
            Ok(())
        })
    }

    pub fn update(&mut self, id: PresetId, input: PresetInput) -> Result<(), RegexStoreError> {
        self.mutate(move |document, path| {
            let index = preset_index(path, document, id)?;
            let name = validate_name(path, &document.presets, &input.name, Some(id))?;
            document.presets[index] = input.into_preset(id, name);
            Ok(())
        })
    }

    pub fn delete(&mut self, id: PresetId) -> Result<(), RegexStoreError> {
        self.mutate(|document, path| {
            let index = preset_index(path, document, id)?;
            document.presets.remove(index);
            Ok(())
        })
    }

    fn mutate<R>(
        &mut self,
        change: impl FnOnce(&mut PresetDocument, &Path) -> Result<R, RegexStoreError>,
    ) -> Result<R, RegexStoreError> {
        self.health.ensure_writable(&self.path)?;
        let (_, mut candidate) = match read_presets(&self.path) {
            Ok(loaded) => loaded,
            Err(error) => {
                self.health.block(&error);
                return Err(error);
            }
        };
        let result = match change(&mut candidate, &self.path) {
            Ok(result) => result,
            Err(error) => {
                self.health.report(&error);
                return Err(error);
            }
        };
        if let Err(error) = save_json_atomic(&self.path, &candidate) {
            self.health.report(&error);
            return Err(error.into());
        }
        self.document = candidate;
        self.health.publish(StoreLoadStatus::Loaded);
        Ok(result)
    }
}

fn invalid(path: &Path, message: impl Into<String>) -> RegexStoreError {
    RegexStoreError::Validation {
        path: path.to_owned(),
        message: message.into(),
    }
}

fn preset_index(
    path: &Path,
    document: &PresetDocument,
    id: PresetId,
) -> Result<usize, RegexStoreError> {
    document
        .presets
        .iter()
        .position(|preset| preset.id == id)
        .ok_or_else(|| invalid(path, format!("preset {} does not exist", id.value())))
}

fn validate_name(
    path: &Path,
    presets: &[RegexPreset],
    name: &str,
    except: Option<PresetId>,
) -> Result<String, RegexStoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid(path, "preset name must not be blank"));
    }
    let folded = name.to_lowercase();
    if presets
        .iter()
        .any(|preset| Some(preset.id) != except && preset.name.to_lowercase() == folded)
    {
        return Err(invalid(
            path,
            format!("preset name {name:?} already exists (case-insensitive)"),
        ));
    }
    Ok(name.to_owned())
}

fn read_presets(path: &Path) -> Result<(StoreLoadStatus, PresetDocument), RegexStoreError> {
    let mut document = match load_json::<PresetDocument>(path)? {
        LoadState::Missing => return Ok((StoreLoadStatus::Missing, PresetDocument::default())),
        LoadState::Empty => return Ok((StoreLoadStatus::Empty, PresetDocument::default())),
        LoadState::Loaded(document) => document,
    };
    if document.version != PRESET_VERSION {
        return Err(RegexStoreError::UnsupportedVersion {
            path: path.to_owned(),
            version: document.version,
        });
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for preset in &mut document.presets {
        if !ids.insert(preset.id) {
            return Err(invalid(path, "duplicate preset identifier"));
        }
        preset.name = preset.name.trim().to_owned();
        if preset.name.is_empty() {
            return Err(invalid(path, "preset name must not be blank"));
        }
        if !names.insert(preset.name.to_lowercase()) {
            return Err(invalid(path, "duplicate case-insensitive preset name"));
        }
        if document.next_id <= preset.id.value() {
            return Err(invalid(
                path,
                "next_id must exceed every existing preset identifier",
            ));
        }
    }
    if document.next_id == 0 {
        return Err(invalid(path, "next_id must be nonzero"));
    }
    Ok((StoreLoadStatus::Loaded, document))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> PresetInput {
        PresetInput {
            name: name.into(),
            pattern: "[unfinished".into(),
            flags: RegexFlags::default(),
            sample_text: None,
            replacement: Some(String::new()),
        }
    }

    fn write_document(path: &Path, document: &PresetDocument) {
        save_json_atomic(path, document).unwrap();
    }

    #[test]
    fn crud_roundtrip_preserves_ids_optional_content_and_incomplete_patterns() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("presets.json");
        let mut store = PresetStore::open(&path);
        assert_eq!(store.load_status(), StoreLoadStatus::Missing);
        assert!(!path.exists());
        let first = store.create(input(" First ")).unwrap();
        assert_eq!(first.value(), 1);
        let mut loaded = PresetStore::open(&path);
        assert_eq!(loaded.get(first).unwrap().name, "First");
        assert_eq!(loaded.get(first).unwrap().sample_text, None);
        assert_eq!(loaded.get(first).unwrap().replacement, Some(String::new()));
        loaded.rename(first, " Renamed ").unwrap();
        let mut changed = input("Updated");
        changed.sample_text = Some(String::new());
        changed.replacement = None;
        changed.flags.case_insensitive = true;
        loaded.update(first, changed).unwrap();
        let updated = PresetStore::open(&path);
        let updated = updated.get(first).unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.sample_text, Some(String::new()));
        assert_eq!(updated.replacement, None);
        assert!(updated.flags.case_insensitive);
        let second = loaded.create(input("Second")).unwrap();
        assert_eq!(second.value(), 2);
        loaded.delete(first).unwrap();
        let third = loaded.create(input("Third")).unwrap();
        assert_eq!(third.value(), 3);
        assert!(loaded.get(first).is_none());
        let fresh = PresetStore::open(&path);
        assert_eq!(fresh.entries(), loaded.entries());
        assert_eq!(fresh.get(second).unwrap().pattern, "[unfinished");
    }

    #[test]
    fn invalid_names_collisions_and_missing_ids_do_not_change_disk_or_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("presets.json");
        let mut store = PresetStore::open(&path);
        let first = store.create(input("One")).unwrap();
        let second = store.create(input("Two")).unwrap();
        let original = std::fs::read(&path).unwrap();
        let snapshot = store.entries().to_vec();
        assert!(store.create(input(" ")).is_err());
        assert!(store.create(input(" ONE ")).is_err());
        assert!(store.rename(second, "one").is_err());
        assert!(store.update(second, input("oNe")).is_err());
        let missing = PresetId::new(99).unwrap();
        assert!(store.rename(missing, "Unused").is_err());
        assert!(store.update(missing, input("Unused")).is_err());
        assert!(store.delete(missing).is_err());
        assert_eq!(store.entries(), snapshot);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        store.rename(first, "ONE").unwrap();
        assert_eq!(store.get(first).unwrap().name, "ONE");
        assert!(store.diagnostic().is_none());
    }

    #[test]
    fn malformed_and_semantically_invalid_storage_block_every_mutation_until_reload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("presets.json");
        let preset = input("One").into_preset(PresetId::new(1).unwrap(), "One".into());
        let valid = PresetDocument {
            version: PRESET_VERSION,
            next_id: 2,
            presets: vec![preset.clone()],
        };
        let mut invalid_documents = vec![b"{bad".to_vec(), br#"{"version":1,"next_id":1,"presets":[{"id":0,"name":"a","pattern":"a","flags":{}}]}"#.to_vec()];
        for document in [
            PresetDocument {
                version: 2,
                next_id: 2,
                presets: vec![],
            },
            PresetDocument {
                version: 1,
                next_id: 0,
                presets: vec![],
            },
            PresetDocument {
                version: 1,
                next_id: 1,
                presets: vec![preset.clone()],
            },
            PresetDocument {
                version: 1,
                next_id: 3,
                presets: vec![preset.clone(), preset.clone()],
            },
            PresetDocument {
                version: 1,
                next_id: 3,
                presets: vec![
                    preset.clone(),
                    input("oNE").into_preset(PresetId::new(2).unwrap(), "oNE".into()),
                ],
            },
            PresetDocument {
                version: 1,
                next_id: 2,
                presets: vec![input(" ").into_preset(PresetId::new(1).unwrap(), " ".into())],
            },
        ] {
            invalid_documents.push(serde_json::to_vec(&document).unwrap());
        }
        for bytes in invalid_documents {
            write_document(&path, &valid);
            let mut store = PresetStore::open(&path);
            std::fs::write(&path, &bytes).unwrap();
            let failed_initial_load = PresetStore::open(&path);
            assert!(!failed_initial_load.is_writable());
            assert!(failed_initial_load.entries().is_empty());
            assert!(failed_initial_load.diagnostic().is_some());
            assert!(store.create(input("New")).is_err());
            assert!(!store.is_writable());
            assert!(store.rename(preset.id, "New").is_err());
            assert!(store.update(preset.id, input("New")).is_err());
            assert!(store.delete(preset.id).is_err());
            assert!(store.reload().is_err());
            assert_eq!(store.entries(), &[preset.clone()]);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            write_document(&path, &valid);
            assert!(matches!(
                store.create(input("New")),
                Err(RegexStoreError::ReloadRequired { .. })
            ));
            store.reload().unwrap();
            assert_eq!(store.create(input("New")).unwrap().value(), 2);
        }
    }

    #[test]
    fn allocator_exhaustion_and_external_valid_edits_are_respected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("presets.json");
        let mut store = PresetStore::open(&path);
        write_document(
            &path,
            &PresetDocument {
                version: 1,
                next_id: u64::MAX,
                presets: vec![],
            },
        );
        let original = std::fs::read(&path).unwrap();
        assert!(store.create(input("No room")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        write_document(
            &path,
            &PresetDocument {
                version: 1,
                next_id: 8,
                presets: vec![
                    input("External").into_preset(PresetId::new(7).unwrap(), "External".into()),
                ],
            },
        );
        assert_eq!(store.create(input("New")).unwrap().value(), 8);
        assert_eq!(store.entries().len(), 2);
        assert_eq!(store.entries()[0].name, "External");
    }

    #[cfg(windows)]
    #[test]
    fn failed_atomic_save_preserves_snapshot_and_allocator_for_retry() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("presets.json");
        let mut store = PresetStore::open(&path);
        let first = store.create(input("One")).unwrap();
        let original = std::fs::read(&path).unwrap();
        let snapshot = store.entries().to_vec();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        assert!(store.create(input("Two")).is_err());
        assert!(store.rename(first, "Renamed").is_err());
        assert!(store.update(first, input("Updated")).is_err());
        assert!(store.delete(first).is_err());
        assert_eq!(store.entries(), snapshot);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        drop(lock);
        assert_eq!(store.create(input("Two")).unwrap().value(), 2);
        assert!(store.diagnostic().is_none());
    }
}
