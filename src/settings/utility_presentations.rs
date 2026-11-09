//! Independently owned presentation preferences for stable utility kinds.
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
    Settings::update_if_changed(path, |settings| {
        settings.utility_presentations.set(key, value)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
