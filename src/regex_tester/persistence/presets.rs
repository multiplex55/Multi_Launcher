use super::{RegexStoreError, StoreDiagnostic, StoreHealth, StoreLoadStatus};
use crate::common::persistence::{LoadState, PersistenceError, read_bytes, save_json_atomic};
use crate::regex_tester::model::{RegexFlags, RegexTestCases};
use crate::regex_tester::policy::{MAX_TEST_CASE_AGGREGATE_TEXT_BYTES, MAX_TEXT_BYTES};
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

const LEGACY_PRESET_VERSION: u32 = 1;
const PRESET_VERSION: u32 = 2;

struct PresetVersionProbe(u32);

impl<'de> Deserialize<'de> for PresetVersionProbe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VersionVisitor;

        impl<'de> Visitor<'de> for VersionVisitor {
            type Value = PresetVersionProbe;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a regex preset document object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut version = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "version" {
                        if version.is_some() {
                            return Err(serde::de::Error::duplicate_field("version"));
                        }
                        version = Some(map.next_value::<u32>()?);
                    } else {
                        let _ = map.next_value::<IgnoredAny>()?;
                    }
                }
                version
                    .map(PresetVersionProbe)
                    .ok_or_else(|| serde::de::Error::missing_field("version"))
            }
        }

        deserializer.deserialize_map(VersionVisitor)
    }
}

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
    pub test_cases: RegexTestCases,
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
    pub test_cases: RegexTestCases,
}

/// Content-only update payload. The current saved name and stable ID are
/// preserved inside the same disk-candidate mutation as the content edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetContent {
    pub pattern: String,
    pub flags: RegexFlags,
    pub sample_text: Option<String>,
    pub replacement: Option<String>,
    pub test_cases: RegexTestCases,
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
            test_cases: self.test_cases,
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

/// The exact pre-case schema. Keeping a separate strict decoder prevents a
/// v1 document from accidentally accepting fields introduced by later
/// versions while still allowing it to load without rewriting its bytes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPresetDocument {
    version: u32,
    next_id: u64,
    presets: Vec<LegacyRegexPreset>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRegexPreset {
    id: PresetId,
    name: String,
    pattern: String,
    flags: RegexFlags,
    sample_text: Option<String>,
    replacement: Option<String>,
}

impl From<LegacyPresetDocument> for PresetDocument {
    fn from(document: LegacyPresetDocument) -> Self {
        Self {
            version: PRESET_VERSION,
            next_id: document.next_id,
            presets: document
                .presets
                .into_iter()
                .map(|preset| RegexPreset {
                    id: preset.id,
                    name: preset.name,
                    pattern: preset.pattern,
                    flags: preset.flags,
                    sample_text: preset.sample_text,
                    replacement: preset.replacement,
                    test_cases: RegexTestCases::default(),
                })
                .collect(),
        }
    }
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

    pub fn update_content(
        &mut self,
        id: PresetId,
        content: PresetContent,
    ) -> Result<(), RegexStoreError> {
        self.mutate(move |document, path| {
            let index = preset_index(path, document, id)?;
            let preset = &mut document.presets[index];
            preset.pattern = content.pattern;
            preset.flags = content.flags;
            preset.sample_text = content.sample_text;
            preset.replacement = content.replacement;
            preset.test_cases = content.test_cases;
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
        for preset in &candidate.presets {
            if let Err(error) = validate_test_cases(&self.path, &preset.test_cases) {
                self.health.report(&error);
                return Err(error);
            }
        }
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
    let bytes = match read_bytes(path)? {
        LoadState::Missing => return Ok((StoreLoadStatus::Missing, PresetDocument::default())),
        LoadState::Empty => return Ok((StoreLoadStatus::Empty, PresetDocument::default())),
        LoadState::Loaded(bytes) => bytes,
    };
    let version = serde_json::from_slice::<PresetVersionProbe>(&bytes)
        .map_err(|source| PersistenceError::MalformedJson {
            path: path.to_owned(),
            source,
        })?
        .0;
    let mut document = match version {
        LEGACY_PRESET_VERSION => {
            let legacy: LegacyPresetDocument =
                serde_json::from_slice(&bytes).map_err(|source| {
                    PersistenceError::MalformedJson {
                        path: path.to_owned(),
                        source,
                    }
                })?;
            if legacy.version != LEGACY_PRESET_VERSION {
                return Err(invalid(
                    path,
                    "version 1 document has an inconsistent version",
                ));
            }
            legacy.into()
        }
        PRESET_VERSION => serde_json::from_slice::<PresetDocument>(&bytes).map_err(|source| {
            PersistenceError::MalformedJson {
                path: path.to_owned(),
                source,
            }
        })?,
        version => {
            return Err(RegexStoreError::UnsupportedVersion {
                path: path.to_owned(),
                version,
            });
        }
    };
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
        validate_test_cases(path, &preset.test_cases)?;
    }
    if document.next_id == 0 {
        return Err(invalid(path, "next_id must be nonzero"));
    }
    Ok((StoreLoadStatus::Loaded, document))
}

fn validate_test_cases(path: &Path, cases: &RegexTestCases) -> Result<(), RegexStoreError> {
    let mut aggregate_bytes = 0usize;
    for case in cases.iter() {
        if case.input.len() > MAX_TEXT_BYTES {
            return Err(invalid(
                path,
                format!(
                    "test case {} input exceeds the {MAX_TEXT_BYTES}-byte limit",
                    case.id.value()
                ),
            ));
        }
        aggregate_bytes = aggregate_bytes
            .checked_add(case.input.len())
            .ok_or_else(|| invalid(path, "aggregate test case input size overflowed"))?;
        if aggregate_bytes > MAX_TEST_CASE_AGGREGATE_TEXT_BYTES {
            return Err(invalid(
                path,
                format!(
                    "aggregate test case input exceeds the {MAX_TEST_CASE_AGGREGATE_TEXT_BYTES}-byte limit"
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::persistence::load_json;

    fn input(name: &str) -> PresetInput {
        PresetInput {
            name: name.into(),
            pattern: "[unfinished".into(),
            flags: RegexFlags::default(),
            sample_text: None,
            replacement: Some(String::new()),
            test_cases: RegexTestCases::default(),
        }
    }

    fn example_cases() -> RegexTestCases {
        let mut cases = RegexTestCases::default();
        let first = cases
            .add(
                Some("café".into()),
                "match café".into(),
                crate::regex_tester::model::RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let second = cases
            .add(
                None,
                "no match".into(),
                crate::regex_tester::model::RegexTestCaseExpectation::NoMatch,
            )
            .unwrap();
        assert!(cases.move_to(second, 0));
        assert_eq!(cases.as_slice()[1].id.value(), first.value());
        cases
    }

    fn write_document(path: &Path, document: &PresetDocument) {
        save_json_atomic(path, document).unwrap();
    }

    #[test]
    fn legacy_v1_load_is_read_only_until_explicit_mutation_writes_v2() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("presets.json");
        let legacy = br#"{"version":1,"next_id":9,"presets":[{"id":7,"name":"  Legacy  ","pattern":"(?P<word>\u00e9)","flags":{"case_insensitive":true},"sample_text":"","replacement":null}]}"#;
        std::fs::write(&path, legacy).unwrap();

        let mut store = PresetStore::open(&path);
        assert!(store.is_writable());
        assert_eq!(store.load_status(), StoreLoadStatus::Loaded);
        assert_eq!(store.entries().len(), 1);
        let legacy_preset = &store.entries()[0];
        assert_eq!(legacy_preset.id.value(), 7);
        assert_eq!(legacy_preset.name, "Legacy");
        assert_eq!(legacy_preset.pattern, "(?P<word>é)");
        assert!(legacy_preset.flags.case_insensitive);
        assert_eq!(legacy_preset.sample_text, Some(String::new()));
        assert_eq!(legacy_preset.replacement, None);
        assert!(legacy_preset.test_cases.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), legacy);
        assert!(store.rename(PresetId::new(7).unwrap(), " ").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), legacy);

        store.rename(PresetId::new(7).unwrap(), "Renamed").unwrap();
        let migrated = match load_json::<serde_json::Value>(&path).unwrap() {
            LoadState::Loaded(value) => value,
            _ => panic!("explicit mutation leaves a JSON preset document"),
        };
        assert_eq!(migrated["version"], PRESET_VERSION);
        assert_eq!(migrated["next_id"], 9);
        assert_eq!(migrated["presets"][0]["id"], 7);
        assert_eq!(migrated["presets"][0]["pattern"], "(?P<word>é)");
        assert_eq!(migrated["presets"][0]["sample_text"], "");
        assert_eq!(
            migrated["presets"][0]["replacement"],
            serde_json::Value::Null
        );
        assert_eq!(migrated["presets"][0]["test_cases"], serde_json::json!([]));
        assert_eq!(store.create(input("After migration")).unwrap().value(), 9);
    }

    #[test]
    fn authored_case_roundtrip_and_other_preset_mutations_preserve_case_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("presets.json");
        let cases = example_cases();
        let mut store = PresetStore::open(&path);
        let mut first_input = input("Cases");
        first_input.pattern = "café".into();
        first_input.test_cases = cases.clone();
        let first = store.create(first_input).unwrap();
        let second = store.create(input("Other")).unwrap();

        let mut reloaded = PresetStore::open(&path);
        assert_eq!(reloaded.get(first).unwrap().test_cases, cases);
        assert_eq!(
            reloaded.get(first).unwrap().test_cases.as_slice()[0].expected,
            crate::regex_tester::model::RegexTestCaseExpectation::NoMatch
        );
        assert_eq!(
            reloaded.get(first).unwrap().test_cases.as_slice()[1]
                .label
                .as_deref(),
            Some("café")
        );

        PresetStore::open(&path)
            .rename(first, "Externally renamed")
            .unwrap();
        let mut updated_cases = cases.clone();
        let first_case = updated_cases.as_slice()[0].id;
        updated_cases.get_mut(first_case).unwrap().input = "edited UTF-8 é".into();
        reloaded
            .update_content(
                first,
                PresetContent {
                    pattern: "new pattern".into(),
                    flags: RegexFlags {
                        case_insensitive: true,
                        ..Default::default()
                    },
                    sample_text: Some(String::new()),
                    replacement: None,
                    test_cases: updated_cases.clone(),
                },
            )
            .unwrap();
        assert_eq!(reloaded.get(first).unwrap().name, "Externally renamed");
        assert_eq!(reloaded.get(first).unwrap().test_cases, updated_cases);
        reloaded.delete(second).unwrap();

        let fresh = PresetStore::open(&path);
        assert_eq!(fresh.entries().len(), 1);
        assert_eq!(fresh.get(first).unwrap().test_cases, updated_cases);
        assert_eq!(fresh.get(first).unwrap().pattern, "new pattern");
        assert!(fresh.get(second).is_none());
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
        changed.test_cases = example_cases();
        changed.flags.case_insensitive = true;
        let expected_cases = changed.test_cases.clone();
        loaded.update(first, changed).unwrap();
        let updated = PresetStore::open(&path);
        let updated = updated.get(first).unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.sample_text, Some(String::new()));
        assert_eq!(updated.replacement, None);
        assert_eq!(updated.test_cases, expected_cases);
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
        let mut oversized_cases = RegexTestCases::default();
        oversized_cases
            .add(
                None,
                "x".repeat(MAX_TEXT_BYTES + 1),
                crate::regex_tester::model::RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let mut oversized = input("Too large");
        oversized.test_cases = oversized_cases;
        assert!(store.create(oversized).is_err());
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
        for flags in [
            serde_json::json!({"case_insensitve": true}),
            serde_json::json!({"unicode": true, "future_flag": true}),
        ] {
            let mut document = serde_json::to_value(&valid).unwrap();
            document["presets"][0]["flags"] = flags;
            invalid_documents.push(serde_json::to_vec(&document).unwrap());
        }
        let mut cases = RegexTestCases::default();
        cases
            .add(
                Some("valid".into()),
                "valid input".into(),
                crate::regex_tester::model::RegexTestCaseExpectation::Match,
            )
            .unwrap();
        cases
            .add(
                None,
                "other input".into(),
                crate::regex_tester::model::RegexTestCaseExpectation::NoMatch,
            )
            .unwrap();
        let mut valid_with_cases = preset.clone();
        valid_with_cases.test_cases = cases;
        let valid_case_document = PresetDocument {
            version: PRESET_VERSION,
            next_id: 2,
            presets: vec![valid_with_cases],
        };
        let mut duplicate_case_ids = serde_json::to_value(&valid_case_document).unwrap();
        duplicate_case_ids["presets"][0]["test_cases"][1]["id"] =
            duplicate_case_ids["presets"][0]["test_cases"][0]["id"].clone();
        invalid_documents.push(serde_json::to_vec(&duplicate_case_ids).unwrap());
        let mut unknown_case_field = serde_json::to_value(&valid_case_document).unwrap();
        unknown_case_field["presets"][0]["test_cases"][0]["future"] = true.into();
        invalid_documents.push(serde_json::to_vec(&unknown_case_field).unwrap());
        let mut malformed_expectation = serde_json::to_value(&valid_case_document).unwrap();
        malformed_expectation["presets"][0]["test_cases"][0]["expected"] = "sometimes".into();
        invalid_documents.push(serde_json::to_vec(&malformed_expectation).unwrap());
        let mut oversized_input = serde_json::to_value(&valid_case_document).unwrap();
        oversized_input["presets"][0]["test_cases"][0]["input"] =
            "x".repeat(MAX_TEXT_BYTES + 1).into();
        invalid_documents.push(serde_json::to_vec(&oversized_input).unwrap());
        let mut aggregate_overflow = RegexTestCases::default();
        for index in 0..4 {
            aggregate_overflow
                .add(
                    Some(format!("case {index}")),
                    "x".repeat(MAX_TEXT_BYTES),
                    crate::regex_tester::model::RegexTestCaseExpectation::Match,
                )
                .unwrap();
        }
        aggregate_overflow
            .add(
                None,
                "x".into(),
                crate::regex_tester::model::RegexTestCaseExpectation::NoMatch,
            )
            .unwrap();
        let mut aggregate_document = serde_json::to_value(&valid_case_document).unwrap();
        aggregate_document["presets"][0]["test_cases"] =
            serde_json::to_value(aggregate_overflow).unwrap();
        invalid_documents.push(serde_json::to_vec(&aggregate_document).unwrap());
        let mut too_many_cases = serde_json::to_value(&valid_case_document).unwrap();
        let cases = too_many_cases["presets"][0]["test_cases"]
            .as_array_mut()
            .unwrap();
        let repeated_case = cases[0].clone();
        cases.resize(crate::regex_tester::MAX_TEST_CASES + 1, repeated_case);
        invalid_documents.push(serde_json::to_vec(&too_many_cases).unwrap());
        let mut unknown_document_field = serde_json::to_value(&valid_case_document).unwrap();
        unknown_document_field["future"] = true.into();
        invalid_documents.push(serde_json::to_vec(&unknown_document_field).unwrap());
        let mut unsupported_version = serde_json::to_value(&valid).unwrap();
        unsupported_version["version"] = (PRESET_VERSION + 1).into();
        invalid_documents.push(serde_json::to_vec(&unsupported_version).unwrap());
        let mut legacy_with_v2_field = serde_json::to_value(&valid).unwrap();
        legacy_with_v2_field["version"] = LEGACY_PRESET_VERSION.into();
        invalid_documents.push(serde_json::to_vec(&legacy_with_v2_field).unwrap());
        invalid_documents.push(br#"{"version":2,"version":2,"next_id":2,"presets":[]}"#.to_vec());
        invalid_documents.push(br#"{"version":2,"next_id":2,"presets":[{"id":1,"name":"One","pattern":"first","pattern":"second","flags":{},"sample_text":null,"replacement":null,"test_cases":[]}] }"#.to_vec());
        for document in [
            PresetDocument {
                version: PRESET_VERSION + 1,
                next_id: 2,
                presets: vec![],
            },
            PresetDocument {
                version: PRESET_VERSION,
                next_id: 0,
                presets: vec![],
            },
            PresetDocument {
                version: PRESET_VERSION,
                next_id: 1,
                presets: vec![preset.clone()],
            },
            PresetDocument {
                version: PRESET_VERSION,
                next_id: 3,
                presets: vec![preset.clone(), preset.clone()],
            },
            PresetDocument {
                version: PRESET_VERSION,
                next_id: 3,
                presets: vec![
                    preset.clone(),
                    input("oNE").into_preset(PresetId::new(2).unwrap(), "oNE".into()),
                ],
            },
            PresetDocument {
                version: PRESET_VERSION,
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
            assert_eq!(store.entries(), std::slice::from_ref(&preset));
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
                version: LEGACY_PRESET_VERSION,
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
                version: PRESET_VERSION,
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
        let mut first_input = input("One");
        first_input.test_cases = example_cases();
        let first = store.create(first_input).unwrap();
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
