//! Checked, sparse persistence of the six Omni Search categories.
use super::*;
use crate::plugins::omni_search::OmniSearchSettings;
use std::io::Write;

const KEYS: [&str; 6] = [
    "include_apps",
    "include_notes",
    "include_todos",
    "include_calendar",
    "include_folders",
    "include_bookmarks",
];
#[derive(Clone, Debug, PartialEq, Eq)]
enum OwnedShape {
    Missing,
    Object(Vec<Option<String>>),
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmniPreferencesSnapshot {
    pub values: OmniSearchSettings,
    owned: OwnedShape,
    positional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmniPreferencesAcknowledgement {
    pub snapshot: OmniPreferencesSnapshot,
    pub persisted: bool,
}

/// The candidate was installed, but its exact durable bytes could not be verified.
#[derive(Debug)]
pub struct OmniCommittedUnverified(pub String);
impl std::fmt::Display for OmniCommittedUnverified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Omni Search save committed but could not be verified: {}",
            self.0
        )
    }
}
impl std::error::Error for OmniCommittedUnverified {}

enum Root {
    Object(RawSettingsObject),
    Positional(Vec<Box<serde_json::value::RawValue>>),
}

pub(super) fn unique_object(text: &str) -> anyhow::Result<RawSettingsObject> {
    use serde::de::{Deserialize, Deserializer, Error, MapAccess, Visitor};
    struct Unique(RawSettingsObject);
    impl<'de> Deserialize<'de> for Unique {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct ObjectVisitor;
            impl<'de> Visitor<'de> for ObjectVisitor {
                type Value = Unique;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("an object with unique keys")
                }
                fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Unique, M::Error> {
                    let mut result = RawSettingsObject::new();
                    while let Some((key, value)) =
                        map.next_entry::<String, Box<serde_json::value::RawValue>>()?
                    {
                        if result.insert(key, value).is_some() {
                            return Err(M::Error::custom("duplicate Settings member"));
                        }
                    }
                    Ok(Unique(result))
                }
            }
            deserializer.deserialize_map(ObjectVisitor)
        }
    }
    Ok(serde_json::from_str::<Unique>(text)?.0)
}

pub(super) fn ordered_fields(
    settings: &Settings,
) -> anyhow::Result<Vec<(String, Box<serde_json::value::RawValue>)>> {
    use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
    struct Fields(Vec<(String, Box<serde_json::value::RawValue>)>);
    impl<'de> Deserialize<'de> for Fields {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Fields;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("serialized Settings")
                }
                fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Fields, M::Error> {
                    let mut fields = Vec::new();
                    while let Some(entry) =
                        map.next_entry::<String, Box<serde_json::value::RawValue>>()?
                    {
                        fields.push(entry);
                    }
                    Ok(Fields(fields))
                }
            }
            deserializer.deserialize_map(V)
        }
    }
    Ok(serde_json::from_slice::<Fields>(&serde_json::to_vec(settings)?)?.0)
}

fn source(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn parse_root(bytes: Option<&[u8]>) -> anyhow::Result<Root> {
    match bytes {
        None => Ok(Root::Object(RawSettingsObject::new())),
        Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => {
            Ok(Root::Object(RawSettingsObject::new()))
        }
        Some(bytes)
            if bytes
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace())
                == Some(b'[') =>
        {
            Ok(Root::Positional(serde_json::from_slice(bytes)?))
        }
        Some(bytes) => Ok(Root::Object(serde_json::from_slice(bytes)?)),
    }
}

fn plugins(root: &Root, position: usize) -> anyhow::Result<RawSettingsObject> {
    let raw = match root {
        Root::Object(root) => root.get("plugin_settings"),
        Root::Positional(root) => root.get(position),
    };
    match raw {
        None => Ok(RawSettingsObject::new()),
        Some(raw) => Ok(serde_json::from_str(raw.get())?),
    }
}

fn snapshot(
    path: &Path,
    bytes: Option<&[u8]>,
    root: &Root,
) -> anyhow::Result<OmniPreferencesSnapshot> {
    let settings = parse_effective_settings(path, bytes.unwrap_or_default())?;
    let fields = ordered_fields(&settings)?;
    let position = fields
        .iter()
        .position(|(key, _)| key == "plugin_settings")
        .ok_or_else(|| anyhow::anyhow!("Settings schema has no plugin settings"))?;
    let plugins = plugins(root, position)?;
    let raw = plugins.get("omni_search");
    let values = settings
        .plugin_settings
        .get("omni_search")
        .and_then(|value| serde_json::from_value::<OmniSearchSettings>(value.clone()).ok())
        .unwrap_or_default();
    let owned = match raw {
        None => OwnedShape::Missing,
        Some(raw) => match unique_object(raw.get()) {
            Ok(object) => OwnedShape::Object(
                KEYS.iter()
                    .map(|key| object.get(*key).map(|value| value.get().to_owned()))
                    .collect(),
            ),
            Err(_) => OwnedShape::Other(raw.get().to_owned()),
        },
    };
    Ok(OmniPreferencesSnapshot {
        values,
        owned,
        positional: matches!(root, Root::Positional(_)),
    })
}

impl Settings {
    pub fn omni_preferences_snapshot(
        path: impl AsRef<Path>,
    ) -> anyhow::Result<OmniPreferencesSnapshot> {
        let path = path.as_ref();
        let _transaction = transaction_guard();
        let bytes = source(path)?;
        snapshot(path, bytes.as_deref(), &parse_root(bytes.as_deref())?)
    }

    pub fn save_omni_preferences_checked(
        path: impl AsRef<Path>,
        baseline: &OmniPreferencesSnapshot,
        desired: &OmniSearchSettings,
    ) -> anyhow::Result<OmniPreferencesAcknowledgement> {
        Self::save_omni_preferences_inner(path.as_ref(), baseline, desired, None)
    }

    pub(crate) fn save_omni_preferences_cancellable(
        path: &Path,
        baseline: &OmniPreferencesSnapshot,
        desired: &OmniSearchSettings,
        token: &SettingsBooleanSaveToken,
    ) -> anyhow::Result<OmniPreferencesAcknowledgement> {
        anyhow::ensure!(
            token.mark_waiting_for_settings(),
            "Settings save was canceled"
        );
        Self::save_omni_preferences_inner(path, baseline, desired, Some(token))
    }

    fn save_omni_preferences_inner(
        path: &Path,
        baseline: &OmniPreferencesSnapshot,
        desired: &OmniSearchSettings,
        token: Option<&SettingsBooleanSaveToken>,
    ) -> anyhow::Result<OmniPreferencesAcknowledgement> {
        let _transaction = transaction_guard();
        anyhow::ensure!(
            !token.is_some_and(SettingsBooleanSaveToken::is_canceled),
            "Settings save was canceled"
        );
        let bytes = source(path)?;
        let mut root = parse_root(bytes.as_deref())?;
        let actual = snapshot(path, bytes.as_deref(), &root)?;
        anyhow::ensure!(
            actual == *baseline,
            "Omni Search settings changed; reload before saving"
        );
        if &actual.values == desired {
            anyhow::ensure!(
                !token.is_some_and(SettingsBooleanSaveToken::is_canceled),
                "Settings save was canceled"
            );
            anyhow::ensure!(
                source(path)? == bytes,
                "Settings changed while checking Omni Search; reload and retry"
            );
            return Ok(OmniPreferencesAcknowledgement {
                snapshot: actual,
                persisted: false,
            });
        }
        if let Some(bytes) = bytes.as_deref() {
            if !bytes.iter().all(u8::is_ascii_whitespace) && !matches!(root, Root::Positional(_)) {
                unique_object(std::str::from_utf8(bytes)?)?;
            }
        }
        let settings = parse_effective_settings(path, bytes.as_deref().unwrap_or_default())?;
        let mut fields = ordered_fields(&settings)?;
        let position = fields
            .iter()
            .position(|(key, _)| key == "plugin_settings")
            .ok_or_else(|| anyhow::anyhow!("Settings schema has no plugin settings"))?;
        let mut plugin_object = plugins(&root, position)?;
        let raw_plugins = match &root {
            Root::Object(root) => root.get("plugin_settings"),
            Root::Positional(root) => root.get(position),
        };
        if let Some(raw) = raw_plugins {
            unique_object(raw.get())?;
        }
        let mut omni: RawSettingsObject = match plugin_object.get("omni_search") {
            Some(raw) if raw.get().trim_start().starts_with('{') => unique_object(raw.get())?,
            _ => RawSettingsObject::new(),
        };
        let serialized = serde_json::to_value(desired)?;
        for key in KEYS {
            omni.insert(key.into(), raw_value(&serialized[key])?);
        }
        plugin_object.insert("omni_search".into(), raw_value(&omni)?);
        match &mut root {
            Root::Object(root) => {
                root.insert("plugin_settings".into(), raw_value(&plugin_object)?);
            }
            Root::Positional(root) => {
                while root.len() <= position {
                    let (_, value) = &mut fields[root.len()];
                    root.push(serde_json::value::RawValue::from_string(
                        value.get().to_owned(),
                    )?);
                }
                root[position] = raw_value(&plugin_object)?;
            }
        }
        let candidate = match &root {
            Root::Object(root) => serde_json::to_vec(root)?,
            Root::Positional(root) => serde_json::to_vec(root)?,
        };
        let next = snapshot(path, Some(&candidate), &root)?;
        anyhow::ensure!(
            next.values == *desired,
            "Omni Search Settings verification failed"
        );
        let claimed = std::cell::Cell::new(false);
        let changed = std::cell::Cell::new(false);
        let current = || {
            if source(path).ok().as_ref() != Some(&bytes) {
                changed.set(true);
                return false;
            }
            if !claimed.get() && token.is_some_and(|token| !token.begin_irrevocable_commit()) {
                return false;
            }
            claimed.set(true);
            true
        };
        if bytes.is_some() {
            let result = crate::common::atomic_file::save_atomic_if(path, &candidate, current);
            anyhow::ensure!(
                !changed.get(),
                "Settings changed while saving Omni Search; reload and retry"
            );
            result?;
        } else {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let mut staged = tempfile::NamedTempFile::new_in(parent)?;
            staged.write_all(&candidate)?;
            staged.flush()?;
            staged.as_file().sync_all()?;
            anyhow::ensure!(
                current(),
                "Omni Search settings changed or save was canceled"
            );
            staged
                .persist_noclobber(path)
                .map_err(|error| error.error)?;
        }
        let verified = (|| -> anyhow::Result<OmniPreferencesSnapshot> {
            let committed = source(path)?.ok_or_else(|| anyhow::anyhow!("Settings disappeared"))?;
            anyhow::ensure!(
                committed == candidate,
                "Settings changed after installation"
            );
            let readback = snapshot(path, Some(&committed), &parse_root(Some(&committed))?)?;
            anyhow::ensure!(
                readback == next,
                "Settings readback differs from the installed receipt"
            );
            Ok(readback)
        })();
        let readback = verified.map_err(|error| OmniCommittedUnverified(format!("{error:#}")))?;
        Ok(OmniPreferencesAcknowledgement {
            snapshot: readback,
            persisted: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_omni_fields_follow_typed_read_but_refuse_ambiguous_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let bytes =
            br#"{"plugin_settings":{"omni_search":{"include_apps":true,"include_apps":false}}}"#;
        std::fs::write(&path, bytes).unwrap();
        let baseline = Settings::omni_preferences_snapshot(&path).unwrap();
        assert!(!baseline.values.include_apps);
        let mut desired = baseline.values.clone();
        desired.include_notes = false;
        assert!(Settings::save_omni_preferences_checked(&path, &baseline, &desired).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn short_positional_settings_extend_only_to_plugin_slot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = Settings::default();
        let fields = ordered_fields(&settings).unwrap();
        let position = fields
            .iter()
            .position(|(key, _)| key == "plugin_settings")
            .unwrap();
        let full = super::super::legacy_positional_settings(&settings);
        let mut prefix: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&full).unwrap();
        prefix.truncate(position);
        let before: Vec<_> = prefix.iter().map(|raw| raw.get().to_owned()).collect();
        std::fs::write(&path, serde_json::to_vec(&prefix).unwrap()).unwrap();
        let baseline = Settings::omni_preferences_snapshot(&path).unwrap();
        let mut desired = baseline.values.clone();
        desired.include_apps = false;
        Settings::save_omni_preferences_checked(&path, &baseline, &desired).unwrap();
        let after: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(after.len(), position + 1);
        assert_eq!(
            before,
            after[..position]
                .iter()
                .map(|raw| raw.get().to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn omni_preserves_opaque_siblings_and_detects_group_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"opaque":{"n":123456789012345678901},"plugin_settings":{"other":{"x":2},"omni_search":{"include_notes":false,"future":{"n":123456789012345678901}}}}"#).unwrap();
        let baseline = Settings::omni_preferences_snapshot(&path).unwrap();
        assert!(!baseline.values.include_notes);
        let original = std::fs::read(&path).unwrap();
        assert!(
            !Settings::save_omni_preferences_checked(&path, &baseline, &baseline.values)
                .unwrap()
                .persisted
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let mut desired = baseline.values.clone();
        desired.include_apps = false;
        let ack = Settings::save_omni_preferences_checked(&path, &baseline, &desired).unwrap();
        assert!(ack.persisted);
        assert_eq!(ack.snapshot.values, desired);
        let root = parse_root(Some(&std::fs::read(&path).unwrap())).unwrap();
        let settings = parse_effective_settings(&path, &std::fs::read(&path).unwrap()).unwrap();
        let position = ordered_fields(&settings)
            .unwrap()
            .iter()
            .position(|(key, _)| key == "plugin_settings")
            .unwrap();
        let plugins = plugins(&root, position).unwrap();
        let omni: RawSettingsObject = serde_json::from_str(plugins["omni_search"].get()).unwrap();
        assert_eq!(omni["future"].get(), r#"{"n":123456789012345678901}"#);
        assert_eq!(plugins["other"].get(), r#"{"x":2}"#);
        assert!(Settings::save_omni_preferences_checked(&path, &baseline, &desired).is_err());
    }

    #[test]
    fn malformed_omni_defaults_without_implicit_rewrite_and_preserves_unknowns_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let bytes = br#"{"plugin_settings":{"omni_search":{"include_apps":"invalid","include_notes":false,"future":42}}}"#;
        std::fs::write(&path, bytes).unwrap();
        let baseline = Settings::omni_preferences_snapshot(&path).unwrap();
        assert_eq!(baseline.values, OmniSearchSettings::default());
        assert!(
            !Settings::save_omni_preferences_checked(&path, &baseline, &baseline.values)
                .unwrap()
                .persisted
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let mut desired = baseline.values.clone();
        desired.include_todos = false;
        let saved = Settings::save_omni_preferences_checked(&path, &baseline, &desired).unwrap();
        assert_eq!(saved.snapshot.values, desired);
        let root = parse_root(Some(&std::fs::read(&path).unwrap())).unwrap();
        let settings = parse_effective_settings(&path, &std::fs::read(&path).unwrap()).unwrap();
        let position = ordered_fields(&settings)
            .unwrap()
            .iter()
            .position(|(key, _)| key == "plugin_settings")
            .unwrap();
        let plugins = plugins(&root, position).unwrap();
        let omni: RawSettingsObject = serde_json::from_str(plugins["omni_search"].get()).unwrap();
        assert_eq!(omni["future"].get(), "42");
    }

    #[test]
    fn canceled_group_and_positional_settings_keep_other_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings
            .plugin_settings
            .insert("other".into(), serde_json::json!({"keep": true}));
        let original = super::super::legacy_positional_settings(&settings);
        std::fs::write(&path, &original).unwrap();
        let baseline = Settings::omni_preferences_snapshot(&path).unwrap();
        let mut desired = baseline.values.clone();
        desired.include_calendar = false;
        let token = SettingsBooleanSaveToken::new(1);
        assert!(token.request_cancel());
        assert!(
            Settings::save_omni_preferences_cancellable(&path, &baseline, &desired, &token)
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let ack = Settings::save_omni_preferences_checked(&path, &baseline, &desired).unwrap();
        assert_eq!(ack.snapshot.values, desired);
        let before: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&original).unwrap();
        let after: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(before.len(), after.len());
        for index in 0..before.len() {
            if index
                != ordered_fields(&settings)
                    .unwrap()
                    .iter()
                    .position(|(key, _)| key == "plugin_settings")
                    .unwrap()
            {
                assert_eq!(before[index].get(), after[index].get());
            }
        }
        assert_eq!(
            Settings::read_current(&path).unwrap().plugin_settings["other"],
            serde_json::json!({"keep": true})
        );
    }
}
