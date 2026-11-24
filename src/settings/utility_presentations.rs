//! Independently owned presentation preferences for stable utility kinds.
use super::store::{raw_object, raw_value};
use super::{Settings, UtilityToolKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;

const VERSION: u32 = 1;
const MAX_TOOLS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RememberedPresentation {
    Docked,
    Popout,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    entries: BTreeMap<UtilityToolKey, RememberedPresentation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UtilityPresentations(Result<Envelope, Value>);

impl Default for UtilityPresentations {
    fn default() -> Self {
        Self(Ok(Envelope {
            version: VERSION,
            entries: BTreeMap::new(),
        }))
    }
}

impl Serialize for UtilityPresentations {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Ok(value) => value.serialize(serializer),
            Err(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for UtilityPresentations {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let parsed = serde_json::from_value::<Envelope>(value.clone())
            .ok()
            .filter(|entry| entry.version == VERSION && entry.entries.len() <= MAX_TOOLS);
        Ok(Self(parsed.ok_or(value)))
    }
}

impl UtilityPresentations {
    pub fn get(&self, key: UtilityToolKey) -> Option<RememberedPresentation> {
        self.0.as_ref().ok()?.entries.get(&key).copied()
    }

    pub fn is_supported(&self) -> bool {
        self.0.is_ok()
    }

    pub(crate) fn set(
        &mut self,
        key: UtilityToolKey,
        value: RememberedPresentation,
    ) -> anyhow::Result<bool> {
        let envelope = self.0.as_mut().map_err(|_| {
            anyhow::anyhow!("unsupported utility presentation settings were preserved")
        })?;
        if envelope.entries.get(&key) == Some(&value) {
            return Ok(false);
        }
        anyhow::ensure!(
            envelope.entries.contains_key(&key) || envelope.entries.len() < MAX_TOOLS,
            "utility presentation preference limit reached"
        );
        envelope.entries.insert(key, value);
        Ok(true)
    }
}

pub(crate) fn persist_presentation(
    path: &str,
    key: UtilityToolKey,
    value: RememberedPresentation,
) -> anyhow::Result<()> {
    Settings::update_utility_envelope(
        path,
        "utility_presentations",
        |settings| Ok((settings.utility_presentations.set(key, value)?, ())),
        |existing, settings| {
            let Some(existing) = existing else {
                return raw_value(&settings.utility_presentations);
            };
            let mut envelope = raw_object(existing)?;
            let mut entries = raw_object(
                envelope
                    .get("entries")
                    .ok_or_else(|| anyhow::anyhow!("utility presentation entries are missing"))?,
            )?;
            entries.insert(key.as_str().to_owned(), raw_value(&value)?);
            envelope.insert("entries".to_owned(), raw_value(&entries)?);
            raw_value(&envelope)
        },
        |intended, candidate| intended.utility_presentations == candidate.utility_presentations,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_write_preserves_raw_siblings_and_noop_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let path_str = path.to_str().unwrap();
        let source = r#"{ "opaque": { "large": 1234567890123456789012345 }, "plugin_settings": { "future": { "x": 1e+10 } }, "utility_dialog_sizes": { "version": 99, "entries": { "future": [ 1, 2 ] } }, "utility_presentations": { "version": 1, "entries": { "hash-generator": "docked", "batch-rename": "p\u006fpout" } } }"#;
        std::fs::write(&path, source).unwrap();
        let key = UtilityToolKey::parse("hash-generator").unwrap();
        persist_presentation(path_str, key, RememberedPresentation::Docked).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
        persist_presentation(path_str, key, RememberedPresentation::Popout).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let root: super::super::store::RawSettingsObject = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            root["opaque"].get(),
            r#"{ "large": 1234567890123456789012345 }"#
        );
        assert_eq!(
            root["plugin_settings"].get(),
            r#"{ "future": { "x": 1e+10 } }"#
        );
        assert_eq!(
            root["utility_dialog_sizes"].get(),
            r#"{ "version": 99, "entries": { "future": [ 1, 2 ] } }"#
        );
        let envelope = raw_object(&root["utility_presentations"]).unwrap();
        let entries = raw_object(&envelope["entries"]).unwrap();
        assert_eq!(entries["batch-rename"].get(), r#""p\u006fpout""#);
        assert_eq!(
            Settings::load(path_str)
                .unwrap()
                .utility_presentations
                .get(key),
            Some(RememberedPresentation::Popout)
        );
    }

    #[test]
    fn presentation_write_handles_sparse_missing_and_rejected_sources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let path_str = path.to_str().unwrap();
        let key = UtilityToolKey::parse("hash-generator").unwrap();
        std::fs::write(&path, br#"{ "opaque": [ 1, 2 ] }"#).unwrap();
        persist_presentation(path_str, key, RememberedPresentation::Docked).unwrap();
        let root: super::super::store::RawSettingsObject =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(root.len(), 2);
        assert_eq!(root["opaque"].get(), "[ 1, 2 ]");
        for source in [
            r#"{"utility_presentations":{"version":99,"entries":{}}}"#,
            "{ invalid",
        ] {
            std::fs::write(&path, source).unwrap();
            assert!(persist_presentation(path_str, key, RememberedPresentation::Popout).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
        }
        std::fs::remove_file(&path).unwrap();
        persist_presentation(path_str, key, RememberedPresentation::Popout).unwrap();
        let root: super::super::store::RawSettingsObject =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(root.len() > 2);
        assert_eq!(
            Settings::load(path_str)
                .unwrap()
                .utility_presentations
                .get(key),
            Some(RememberedPresentation::Popout)
        );
    }

    #[test]
    fn presentation_write_accepts_positional_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let source = super::super::store::legacy_positional_settings(&Settings::default());
        std::fs::write(&path, source).unwrap();
        let key = UtilityToolKey::parse("hash-generator").unwrap();
        persist_presentation(path.to_str().unwrap(), key, RememberedPresentation::Popout).unwrap();
        assert_eq!(
            Settings::load(path.to_str().unwrap())
                .unwrap()
                .utility_presentations
                .get(key),
            Some(RememberedPresentation::Popout)
        );
    }

    #[test]
    fn utility_transaction_rejects_a_candidate_with_the_wrong_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let source = br#"{"opaque":[ 1, 2 ]}"#;
        std::fs::write(&path, source).unwrap();
        let key = UtilityToolKey::parse("hash-generator").unwrap();
        assert!(
            Settings::update_utility_envelope(
                &path,
                "utility_presentations",
                |settings| Ok((
                    settings
                        .utility_presentations
                        .set(key, RememberedPresentation::Popout)?,
                    ()
                )),
                |_, _| raw_value(&UtilityPresentations::default()),
                |intended, candidate| intended.utility_presentations
                    == candidate.utility_presentations,
            )
            .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), source);
    }

    #[cfg(windows)]
    #[test]
    fn presentation_write_reports_atomic_refusal() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let source = br#"{"opaque":[1,2]}"#;
        std::fs::write(&path, source).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(&path)
            .unwrap();
        let key = UtilityToolKey::parse("hash-generator").unwrap();
        assert!(
            persist_presentation(path.to_str().unwrap(), key, RememberedPresentation::Popout)
                .is_err()
        );
        drop(lock);
        assert_eq!(std::fs::read(path).unwrap(), source);
    }

    #[test]
    fn presentation_preferences_default_roundtrip_and_preserve_opaque_data() {
        let key = UtilityToolKey::parse("batch-rename").unwrap();
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.utility_presentations.get(key), None);
        let mut preferences = UtilityPresentations::default();
        assert!(
            preferences
                .set(key, RememberedPresentation::Popout)
                .unwrap()
        );
        assert!(
            !preferences
                .set(key, RememberedPresentation::Popout)
                .unwrap()
        );
        let encoded = serde_json::to_value(&preferences).unwrap();
        let restored: UtilityPresentations = serde_json::from_value(encoded).unwrap();
        assert_eq!(restored.get(key), Some(RememberedPresentation::Popout));
        for value in [
            serde_json::json!({"version": 2, "entries": {}}),
            serde_json::json!({"version": 1, "entries": {"future-tool": "docked"}}),
            serde_json::json!({"version": 1, "entries": {"batch-rename": "future"}}),
            serde_json::json!("malformed"),
        ] {
            let mut opaque: UtilityPresentations = serde_json::from_value(value.clone()).unwrap();
            assert!(!opaque.is_supported());
            assert!(opaque.set(key, RememberedPresentation::Docked).is_err());
            assert_eq!(serde_json::to_value(opaque).unwrap(), value);
        }
    }
}
