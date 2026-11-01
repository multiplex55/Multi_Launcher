//! Bounded, local-only preferred sizes for ordinary utility windows.
//!
//! The persisted representation is tolerant by design: an unknown schema or
//! malformed utility-size subtree is retained as opaque JSON when another
//! settings field is saved, while geometry writes fail closed for that value.

use anyhow::{Context, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

pub const CURRENT_VERSION: u16 = 1;
pub const MAX_ACTIVE_TOOLS: usize = 96;
pub const MIN_DIMENSION: f32 = 64.0;
pub const MAX_DIMENSION: f32 = 8192.0;

const STABLE_TOOL_KEYS: &[&str] = &[
    "action-sheet",
    "activity",
    "add-action",
    "add-bookmark",
    "batch-rename",
    "bookmark-alias",
    "brightness",
    "calendar",
    "calendar-event-details",
    "calendar-event-editor",
    "clipboard-history",
    "clipboard-modify",
    "command-help",
    "cpu-usage",
    "create-tempfile",
    "custom-apps",
    "dashboard-editor",
    "data-recovery",
    "diff",
    "diff-comparison-rules",
    "diff-folder-rules",
    "diff-text-input",
    "duplicate-finder",
    "favorites",
    "file-preview",
    "file-search",
    "folder-alias",
    "gesture-binding-editor",
    "gesture-ignore-window-picker",
    "gesture-settings",
    "gesture-settings-window-picker",
    "hash-generator",
    "hotfolder-manager",
    "hotkey-directory",
    "image-comparison",
    "image-crop-editor",
    "image-viewer",
    "json-utility",
    "legacy-macros",
    "mkmacro",
    "mkmacro-action-catalog",
    "mkmacro-action-editor",
    "mkmacro-help",
    "mkmacro-launcher-picker",
    "mkmacro-package-export",
    "mkmacro-package-import",
    "mkmacro-parameters",
    "mkmacro-recording-review",
    "mkmacro-reference-crop",
    "mkmacro-runtime-prompt",
    "mkmacro-step-search",
    "mkmacro-template-save",
    "mkmacro-templates",
    "mkmacro-window-picker",
    "mouse-gestures",
    "mouse-settings",
    "multi-manager",
    "multi-manager-settings",
    "note-editor",
    "note-graph",
    "note-templates",
    "ocr-result",
    "paste-sequences",
    "plugin-settings",
    "qr-generator",
    "quick-help",
    "quick-notes",
    "regex-tester",
    "screenshot-editor",
    "screenshot-pin-list",
    "search-replace",
    "settings",
    "shell-commands",
    "snippet-prompt",
    "snippets",
    "table-utility",
    "tempfile-alias",
    "theme-settings",
    "toast-log",
    "todos",
    "unicode-inspector",
    "unused-note-assets",
    "volume",
    "xml-toolkit",
    "zip-extractor",
];

/// A validated stable window kind. Keys are never derived from document names,
/// paths, queries, or window titles.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UtilityToolKey(&'static str);

impl UtilityToolKey {
    pub fn parse(value: &str) -> Result<Self, UnknownUtilityTool> {
        STABLE_TOOL_KEYS
            .iter()
            .find(|&&known| known == value)
            .map(|&known| Self(known))
            .ok_or_else(|| UnknownUtilityTool(value.to_owned()))
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Serialize for UtilityToolKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for UtilityToolKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownUtilityTool(String);

impl fmt::Display for UnknownUtilityTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown utility window type '{}'; no size was stored",
            self.0
        )
    }
}

impl std::error::Error for UnknownUtilityTool {}

/// The durable user preference. It remains separate from viewport constraints.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreferredWindowSize {
    pub width: f32,
    pub height: f32,
}

impl PreferredWindowSize {
    pub fn new(width: f32, height: f32) -> anyhow::Result<Self> {
        let size = Self { width, height };
        size.validate()?;
        Ok(size)
    }

    fn validate(self) -> anyhow::Result<()> {
        for (axis, value) in [("width", self.width), ("height", self.height)] {
            anyhow::ensure!(
                value.is_finite() && (MIN_DIMENSION..=MAX_DIMENSION).contains(&value),
                "utility window {axis} must be finite and between {MIN_DIMENSION} and {MAX_DIMENSION} logical points"
            );
        }
        Ok(())
    }

    /// Apply current available bounds without mutating this stored preference.
    pub fn apply_within(self, available_width: f32, available_height: f32) -> AppliedWindowSize {
        AppliedWindowSize {
            width: self.width.min(available_width.max(1.0)),
            height: self.height.min(available_height.max(1.0)),
        }
    }
}

/// The current viewport-constrained size, distinct from the stored preference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppliedWindowSize {
    pub width: f32,
    pub height: f32,
}

/// A frame's observed dimensions. Observations alone do not imply user intent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObservedWindowSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UtilitySizeRevision(pub u64);

/// A successful persistence acknowledgement. Consumers compare its revision
/// with the latest requested revision before publishing it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UtilitySizeAcknowledgement {
    pub key: UtilityToolKey,
    pub revision: UtilitySizeRevision,
    pub preference: UtilitySizePreference,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u16,
    entries: BTreeMap<UtilityToolKey, Entry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum Entry {
    Preferred { size: PreferredWindowSize },
    Reset,
}

#[derive(Clone, Debug, PartialEq)]
enum Representation {
    Supported(Envelope),
    Opaque(Value),
}

/// Versioned utility-size settings with lossless handling of unsupported JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct UtilityDialogSizes(Representation);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UtilitySizePreference {
    Missing,
    Reset,
    Preferred(PreferredWindowSize),
    Unsupported,
}

impl Default for UtilityDialogSizes {
    fn default() -> Self {
        Self(Representation::Supported(Envelope {
            version: CURRENT_VERSION,
            entries: BTreeMap::new(),
        }))
    }
}

impl Serialize for UtilityDialogSizes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.0 {
            Representation::Supported(envelope) => envelope.serialize(serializer),
            Representation::Opaque(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for UtilityDialogSizes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let too_many_entries = value
            .get("entries")
            .and_then(Value::as_object)
            .is_some_and(|entries| entries.len() > MAX_ACTIVE_TOOLS);
        let parsed = (!too_many_entries)
            .then(|| serde_json::from_value::<Envelope>(value.clone()).ok())
            .flatten()
            .filter(|envelope| {
                envelope.version == CURRENT_VERSION
                    && envelope.entries.len() <= MAX_ACTIVE_TOOLS
                    && envelope.entries.values().all(|entry| match entry {
                        Entry::Preferred { size } => size.validate().is_ok(),
                        Entry::Reset => true,
                    })
            });
        Ok(Self(match parsed {
            Some(envelope) => Representation::Supported(envelope),
            None => Representation::Opaque(value),
        }))
    }
}

impl UtilityDialogSizes {
    pub fn preferred(&self, key: UtilityToolKey) -> Option<PreferredWindowSize> {
        match self.preference(key) {
            UtilitySizePreference::Preferred(size) => Some(size),
            UtilitySizePreference::Missing
            | UtilitySizePreference::Reset
            | UtilitySizePreference::Unsupported => None,
        }
    }

    pub fn preference(&self, key: UtilityToolKey) -> UtilitySizePreference {
        match &self.0 {
            Representation::Supported(envelope) => match envelope.entries.get(&key) {
                Some(Entry::Preferred { size }) => UtilitySizePreference::Preferred(*size),
                Some(Entry::Reset) => UtilitySizePreference::Reset,
                None => UtilitySizePreference::Missing,
            },
            Representation::Opaque(_) => UtilitySizePreference::Unsupported,
        }
    }

    pub fn is_supported(&self) -> bool {
        matches!(self.0, Representation::Supported(_))
    }

    pub(crate) fn set(
        &mut self,
        key: UtilityToolKey,
        size: PreferredWindowSize,
    ) -> anyhow::Result<bool> {
        size.validate()?;
        let envelope = self.supported_mut()?;
        let next = Entry::Preferred { size };
        if envelope.entries.get(&key) == Some(&next) {
            return Ok(false);
        }
        if !envelope.entries.contains_key(&key) {
            anyhow::ensure!(
                envelope.entries.len() < MAX_ACTIVE_TOOLS,
                "utility window size limit reached"
            );
        }
        envelope.entries.insert(key, next);
        Ok(true)
    }

    pub(crate) fn reset(&mut self, key: UtilityToolKey) -> anyhow::Result<bool> {
        let envelope = self.supported_mut()?;
        if envelope.entries.get(&key) == Some(&Entry::Reset) {
            return Ok(false);
        }
        if !envelope.entries.contains_key(&key) {
            anyhow::ensure!(
                envelope.entries.len() < MAX_ACTIVE_TOOLS,
                "utility window size limit reached"
            );
        }
        envelope.entries.insert(key, Entry::Reset);
        Ok(true)
    }

    fn supported_mut(&mut self) -> anyhow::Result<&mut Envelope> {
        match &mut self.0 {
            Representation::Supported(envelope) => Ok(envelope),
            Representation::Opaque(_) => bail!(
                "saved utility window sizes use an unsupported or invalid format; preferences were left unchanged"
            ),
        }
    }
}

pub(crate) fn persist_mutation(
    settings_path: &str,
    key: UtilityToolKey,
    mutation: UtilitySizeMutation,
) -> anyhow::Result<UtilitySizePreference> {
    let (_, committed) = crate::settings::Settings::update_if_changed(settings_path, |settings| {
        match mutation {
            UtilitySizeMutation::Set(size) => {
                settings.utility_dialog_sizes.set(key, size)?;
            }
            UtilitySizeMutation::Reset => {
                settings.utility_dialog_sizes.reset(key)?;
            }
        }
        Ok(settings.utility_dialog_sizes.preference(key))
    })
    .context("failed to persist preferred utility window size")?;
    Ok(committed)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UtilitySizeMutation {
    Set(PreferredWindowSize),
    Reset,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn key(value: &str) -> UtilityToolKey {
        UtilityToolKey::parse(value).unwrap()
    }

    #[test]
    fn utility_dialog_sizes_defaults_and_round_trips() {
        let old_settings = serde_json::from_str::<Settings>("{}").unwrap();
        let old = old_settings.utility_dialog_sizes;
        assert!(old.is_supported());
        assert_eq!(old.preferred(key("diff")), None);

        let mut sizes = UtilityDialogSizes::default();
        let preferred = PreferredWindowSize::new(720.0, 480.0).unwrap();
        assert!(sizes.set(key("diff"), preferred).unwrap());
        let json = serde_json::to_string(&sizes).unwrap();
        let restored: UtilityDialogSizes = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.preferred(key("diff")), Some(preferred));
    }

    #[test]
    fn utility_dialog_sizes_reject_invalid_dimensions_and_unknown_tools() {
        for (width, height) in [
            (f32::NAN, 200.0),
            (f32::INFINITY, 200.0),
            (0.0, 200.0),
            (9000.0, 200.0),
            (200.0, -1.0),
        ] {
            assert!(PreferredWindowSize::new(width, height).is_err());
        }
        assert!(UtilityToolKey::parse("user-document-window").is_err());
    }

    #[test]
    fn utility_dialog_sizes_preserve_invalid_and_future_envelopes_opaquely() {
        let mut sources = vec![
            r#"{"version":1,"entries":{"diff":{"state":"preferred","size":{"width":null,"height":300}}}}"#.to_owned(),
            r#"{"version":55,"entries":{"diff":{"state":"reset"}},"future":true}"#.to_owned(),
            r#"{"version":1,"entries":{"future-tool":{"state":"reset"}}}"#.to_owned(),
        ];
        sources.push(format!(
            r#"{{"version":1,"entries":{{{}}}}}"#,
            (0..=MAX_ACTIVE_TOOLS)
                .map(|index| format!(r#""future-{index}":{{"state":"reset"}}"#))
                .collect::<Vec<_>>()
                .join(",")
        ));
        for source in sources {
            let mut settings: Settings = serde_json::from_str(&format!(
                r#"{{"utility_dialog_sizes":{source},"hotkey":"F2"}}"#
            ))
            .unwrap();
            let before = settings.utility_dialog_sizes.clone();
            settings.show_examples = true;
            let encoded = serde_json::to_value(&settings).unwrap();
            let restored: Settings = serde_json::from_value(encoded).unwrap();
            assert_eq!(restored.utility_dialog_sizes, before);
            assert!(!restored.utility_dialog_sizes.is_supported());
            assert_eq!(
                restored.utility_dialog_sizes.preference(key("diff")),
                UtilitySizePreference::Unsupported
            );
            assert!(
                restored
                    .utility_dialog_sizes
                    .clone()
                    .set(key("diff"), PreferredWindowSize::new(640.0, 480.0).unwrap())
                    .is_err()
            );
        }
    }

    #[test]
    fn utility_dialog_sizes_reset_tombstone_and_tools_are_independent() {
        let mut sizes = UtilityDialogSizes::default();
        sizes
            .set(key("diff"), PreferredWindowSize::new(700.0, 500.0).unwrap())
            .unwrap();
        sizes
            .set(
                key("file-preview"),
                PreferredWindowSize::new(800.0, 600.0).unwrap(),
            )
            .unwrap();
        assert!(sizes.reset(key("diff")).unwrap());
        assert_eq!(sizes.preferred(key("diff")), None);
        assert_eq!(sizes.preference(key("diff")), UtilitySizePreference::Reset);
        assert_eq!(sizes.preferred(key("file-preview")).unwrap().width, 800.0);
        assert!(!sizes.reset(key("diff")).unwrap());
    }

    #[test]
    fn utility_dialog_sizes_preserve_preference_when_viewport_clamps() {
        let preferred = PreferredWindowSize::new(1200.0, 900.0).unwrap();
        assert_eq!(
            preferred.apply_within(800.0, 600.0),
            AppliedWindowSize {
                width: 800.0,
                height: 600.0
            }
        );
        assert_eq!(preferred.width, 1200.0);
        let _observed = ObservedWindowSize {
            width: 800.0,
            height: 600.0,
        };
    }

    #[test]
    fn utility_dialog_sizes_settings_editor_mapping_preserves_latest_value() {
        let mut current = Settings::default();
        current
            .utility_dialog_sizes
            .set(key("diff"), PreferredWindowSize::new(700.0, 500.0).unwrap())
            .unwrap();
        let editor = crate::settings_editor::SettingsEditor::from_settings(&current);
        let restored = editor.to_settings(&current).unwrap();
        assert_eq!(restored.utility_dialog_sizes, current.utility_dialog_sizes);
    }

    #[test]
    fn utility_dialog_sizes_persistence_is_coherent_and_skips_noop_writes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        Settings::default().save(path.to_str().unwrap()).unwrap();
        let size = PreferredWindowSize::new(760.0, 520.0).unwrap();

        persist_mutation(
            path.to_str().unwrap(),
            key("diff"),
            UtilitySizeMutation::Set(size),
        )
        .unwrap();
        let committed_bytes = std::fs::read(&path).unwrap();
        let (changed, ()) = Settings::update_if_changed(path.to_str().unwrap(), |settings| {
            settings.utility_dialog_sizes.set(key("diff"), size)?;
            Ok(())
        })
        .unwrap();
        assert!(!changed, "equal settings should bypass the disk writer");
        assert_eq!(std::fs::read(&path).unwrap(), committed_bytes);
        let restored = Settings::load(path.to_str().unwrap()).unwrap();
        assert_eq!(
            restored.utility_dialog_sizes.preferred(key("diff")),
            Some(size)
        );

        persist_mutation(
            path.to_str().unwrap(),
            key("diff"),
            UtilitySizeMutation::Reset,
        )
        .unwrap();
        let reset = Settings::load(path.to_str().unwrap()).unwrap();
        assert_eq!(reset.utility_dialog_sizes.preferred(key("diff")), None);
        let encoded = serde_json::to_value(&reset.utility_dialog_sizes).unwrap();
        assert_eq!(encoded["entries"]["diff"]["state"], "reset");
    }

    #[test]
    fn utility_dialog_sizes_failed_persistence_does_not_publish_a_value() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::create_dir(&path).unwrap();
        let size = PreferredWindowSize::new(720.0, 480.0).unwrap();
        assert!(
            persist_mutation(
                path.to_str().unwrap(),
                key("diff"),
                UtilitySizeMutation::Set(size)
            )
            .is_err()
        );
        assert!(path.is_dir());
    }

    #[test]
    fn utility_dialog_sizes_concurrent_mutations_preserve_each_other_and_settings() {
        use std::sync::{Arc, Barrier};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        Settings::default().save(path.to_str().unwrap()).unwrap();
        let path = Arc::new(path.to_string_lossy().into_owned());
        let barrier = Arc::new(Barrier::new(3));

        let first = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                persist_mutation(
                    &path,
                    key("diff"),
                    UtilitySizeMutation::Set(PreferredWindowSize::new(700.0, 500.0).unwrap()),
                )
                .unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                persist_mutation(
                    &path,
                    key("file-preview"),
                    UtilitySizeMutation::Set(PreferredWindowSize::new(800.0, 600.0).unwrap()),
                )
                .unwrap();
            })
        };
        barrier.wait();
        Settings::update(&path, |settings| {
            settings.show_examples = true;
            Ok(())
        })
        .unwrap();
        first.join().unwrap();
        second.join().unwrap();

        let saved = Settings::load(&path).unwrap();
        assert!(saved.show_examples);
        assert_eq!(
            saved.utility_dialog_sizes.preferred(key("diff")),
            Some(PreferredWindowSize::new(700.0, 500.0).unwrap())
        );
        assert_eq!(
            saved.utility_dialog_sizes.preferred(key("file-preview")),
            Some(PreferredWindowSize::new(800.0, 600.0).unwrap())
        );
    }
}
