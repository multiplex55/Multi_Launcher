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

pub const CURRENT_VERSION: u16 = 2;
pub const MAX_ACTIVE_TOOLS: usize = 96;
pub const MIN_DIMENSION: f32 = 64.0;
pub const MAX_DIMENSION: f32 = 8192.0;
pub const MAX_POSITION_COORDINATE: f32 = 131_072.0;
pub const MIN_SCALE_FACTOR: f32 = 0.5;
pub const MAX_SCALE_FACTOR: f32 = 8.0;

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
    "calculation-pad",
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
    "diagram-studio",
    "diagram-viewer",
    "duplicate-finder",
    "favorites",
    "file-preview",
    "file-lock",
    "file-search",
    "folder-alias",
    "folder-size",
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
    "markdown-studio",
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
    "timer",
    "toast-log",
    "todos",
    "unicode-inspector",
    "unused-note-assets",
    "volume",
    "xml-toolkit",
    "zip-extractor",
    "date-time-studio",
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

/// A desktop position in logical points as observed at `scale_factor`.
/// Negative values are valid for monitors left of or above the primary one.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedWindowPosition {
    pub x: f32,
    pub y: f32,
}

impl SavedWindowPosition {
    pub fn new(x: f32, y: f32) -> anyhow::Result<Self> {
        let position = Self { x, y };
        position.validate()?;
        Ok(position)
    }

    fn validate(self) -> anyhow::Result<()> {
        for (axis, value) in [("x", self.x), ("y", self.y)] {
            anyhow::ensure!(
                value.is_finite() && value.abs() <= MAX_POSITION_COORDINATE,
                "utility window {axis} position must be finite and within ±{MAX_POSITION_COORDINATE} logical desktop points"
            );
        }
        Ok(())
    }
}

/// Native outer geometry, stored in logical desktop points with the observed
/// physical-pixels-per-point scale. A later host can convert those saved
/// coordinates through this scale to the current monitor's scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWindowGeometry {
    pub position: SavedWindowPosition,
    pub size: PreferredWindowSize,
    pub scale_factor: f32,
}

impl NativeWindowGeometry {
    pub fn new(
        position: SavedWindowPosition,
        size: PreferredWindowSize,
        scale_factor: f32,
    ) -> anyhow::Result<Self> {
        let geometry = Self {
            position,
            size,
            scale_factor,
        };
        geometry.validate()?;
        Ok(geometry)
    }

    fn validate(self) -> anyhow::Result<()> {
        self.position.validate()?;
        self.size.validate()?;
        anyhow::ensure!(
            self.scale_factor.is_finite()
                && (MIN_SCALE_FACTOR..=MAX_SCALE_FACTOR).contains(&self.scale_factor),
            "utility window scale factor must be finite and between {MIN_SCALE_FACTOR} and {MAX_SCALE_FACTOR}"
        );
        anyhow::ensure!(
            (self.position.x * self.scale_factor).is_finite()
                && (self.position.y * self.scale_factor).is_finite(),
            "scaled utility window position is out of range"
        );
        Ok(())
    }
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
    /// The durable native geometry after this revision, if present.
    pub native_geometry: Option<NativeWindowGeometry>,
    /// Whether this revision included a docked size or reset intent.
    pub affects_size: bool,
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
    Preferred {
        size: PreferredWindowSize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        native: Option<NativeWindowGeometry>,
    },
    Reset {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        native: Option<NativeWindowGeometry>,
    },
    Geometry {
        native: NativeWindowGeometry,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEnvelopeV1 {
    version: u16,
    entries: BTreeMap<UtilityToolKey, LegacyEntryV1>,
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum LegacyEntryV1 {
    Preferred { size: PreferredWindowSize },
    Reset,
}

impl Entry {
    fn validate(&self) -> bool {
        match self {
            Self::Preferred { size, native } => {
                size.validate().is_ok() && native.is_none_or(|geometry| geometry.validate().is_ok())
            }
            Self::Reset { native } => native.is_none_or(|geometry| geometry.validate().is_ok()),
            Self::Geometry { native } => native.validate().is_ok(),
        }
    }
}

fn deserialize_envelope(value: &Value) -> Option<Envelope> {
    let version = u16::try_from(value.get("version")?.as_u64()?).ok()?;
    match version {
        1 => {
            let legacy = serde_json::from_value::<LegacyEnvelopeV1>(value.clone()).ok()?;
            if legacy.version != 1 || legacy.entries.len() > MAX_ACTIVE_TOOLS {
                return None;
            }
            let entries = legacy
                .entries
                .into_iter()
                .map(|(key, entry)| {
                    let entry = match entry {
                        LegacyEntryV1::Preferred { size } => {
                            Entry::Preferred { size, native: None }
                        }
                        LegacyEntryV1::Reset => Entry::Reset { native: None },
                    };
                    (key, entry)
                })
                .collect();
            Some(Envelope {
                version: CURRENT_VERSION,
                entries,
            })
        }
        CURRENT_VERSION => serde_json::from_value::<Envelope>(value.clone())
            .ok()
            .filter(|envelope| envelope.version == CURRENT_VERSION),
        _ => None,
    }
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
            .then(|| deserialize_envelope(&value))
            .flatten()
            .filter(|envelope| {
                envelope.entries.len() <= MAX_ACTIVE_TOOLS
                    && envelope.entries.values().all(Entry::validate)
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
                Some(Entry::Preferred { size, .. }) => UtilitySizePreference::Preferred(*size),
                Some(Entry::Reset { .. }) => UtilitySizePreference::Reset,
                Some(Entry::Geometry { .. }) | None => UtilitySizePreference::Missing,
            },
            Representation::Opaque(_) => UtilitySizePreference::Unsupported,
        }
    }

    /// Return the stable native outer geometry independently of the docked
    /// preferred size.
    pub fn native_geometry(&self, key: UtilityToolKey) -> Option<NativeWindowGeometry> {
        match &self.0 {
            Representation::Supported(envelope) => match envelope.entries.get(&key) {
                Some(Entry::Preferred { native, .. } | Entry::Reset { native }) => *native,
                Some(Entry::Geometry { native }) => Some(*native),
                None => None,
            },
            Representation::Opaque(_) => None,
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
        let native = match envelope.entries.get(&key) {
            Some(Entry::Preferred { native, .. } | Entry::Reset { native }) => *native,
            Some(Entry::Geometry { native }) => Some(*native),
            None => None,
        };
        let next = Entry::Preferred { size, native };
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
        let next = Entry::Reset { native: None };
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

    pub(crate) fn set_native_geometry(
        &mut self,
        key: UtilityToolKey,
        geometry: NativeWindowGeometry,
    ) -> anyhow::Result<bool> {
        geometry.validate()?;
        let envelope = self.supported_mut()?;
        let next = match envelope.entries.get(&key) {
            Some(Entry::Preferred { size, .. }) => Entry::Preferred {
                size: *size,
                native: Some(geometry),
            },
            Some(Entry::Reset { .. }) => Entry::Reset {
                native: Some(geometry),
            },
            Some(Entry::Geometry { .. }) | None => Entry::Geometry { native: geometry },
        };
        if envelope.entries.get(&key) == Some(&next) {
            return Ok(false);
        }
        if !envelope.entries.contains_key(&key) {
            anyhow::ensure!(
                envelope.entries.len() < MAX_ACTIVE_TOOLS,
                "utility window geometry limit reached"
            );
        }
        envelope.entries.insert(key, next);
        Ok(true)
    }

    /// Replace one key from a successful writer acknowledgement. Unlike the
    /// ordinary component setters, this applies an authoritative snapshot and
    /// removes stale components that are absent from the committed value.
    pub(crate) fn replace_committed(
        &mut self,
        key: UtilityToolKey,
        preference: UtilitySizePreference,
        native: Option<NativeWindowGeometry>,
    ) -> anyhow::Result<bool> {
        let next = match (preference, native) {
            (UtilitySizePreference::Preferred(size), native) => {
                size.validate()?;
                if let Some(native) = native {
                    native.validate()?;
                }
                Some(Entry::Preferred { size, native })
            }
            (UtilitySizePreference::Reset, native) => {
                if let Some(native) = native {
                    native.validate()?;
                }
                Some(Entry::Reset { native })
            }
            (UtilitySizePreference::Missing, Some(native)) => {
                native.validate()?;
                Some(Entry::Geometry { native })
            }
            (UtilitySizePreference::Missing, None) => None,
            (UtilitySizePreference::Unsupported, _) => {
                bail!("cannot publish an unsupported utility geometry snapshot")
            }
        };

        let envelope = self.supported_mut()?;
        if envelope.entries.get(&key) == next.as_ref() {
            return Ok(false);
        }
        if next.is_some() && !envelope.entries.contains_key(&key) {
            anyhow::ensure!(
                envelope.entries.len() < MAX_ACTIVE_TOOLS,
                "utility window geometry limit reached"
            );
        }
        if let Some(next) = next {
            envelope.entries.insert(key, next);
        } else {
            envelope.entries.remove(&key);
        }
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
) -> anyhow::Result<(UtilitySizePreference, Option<NativeWindowGeometry>)> {
    let (_, committed) = crate::settings::Settings::update_if_changed(settings_path, |settings| {
        match mutation {
            UtilitySizeMutation::Set(size) => {
                settings.utility_dialog_sizes.set(key, size)?;
            }
            UtilitySizeMutation::SetNative(geometry) => {
                settings
                    .utility_dialog_sizes
                    .set_native_geometry(key, geometry)?;
            }
            UtilitySizeMutation::Reset => {
                settings.utility_dialog_sizes.reset(key)?;
            }
            UtilitySizeMutation::Components {
                reset,
                size,
                native,
            } => {
                if reset {
                    settings.utility_dialog_sizes.reset(key)?;
                }
                if let Some(size) = size {
                    settings.utility_dialog_sizes.set(key, size)?;
                }
                if let Some(geometry) = native {
                    settings
                        .utility_dialog_sizes
                        .set_native_geometry(key, geometry)?;
                }
            }
        }
        Ok((
            settings.utility_dialog_sizes.preference(key),
            settings.utility_dialog_sizes.native_geometry(key),
        ))
    })
    .context("failed to persist utility window geometry")?;
    Ok(committed)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UtilitySizeMutation {
    Set(PreferredWindowSize),
    SetNative(NativeWindowGeometry),
    Reset,
    Components {
        reset: bool,
        size: Option<PreferredWindowSize>,
        native: Option<NativeWindowGeometry>,
    },
}

impl UtilitySizeMutation {
    /// Merge a later component update with a pending one for the same tool.
    /// A reset starts a new base; subsequent component updates are applied on
    /// top of that reset without reviving the cleared component.
    pub(crate) fn followed_by(self, next: Self) -> Self {
        let (reset, size, native) = self.components();
        let (next_reset, next_size, next_native) = next.components();
        if next_reset {
            return next;
        }
        let reset = reset || next_reset;
        let size = next_size.or(size);
        let native = next_native.or(native);
        match (reset, size, native) {
            (false, Some(size), None) => Self::Set(size),
            (false, None, Some(native)) => Self::SetNative(native),
            (true, None, None) => Self::Reset,
            (reset, size, native) => Self::Components {
                reset,
                size,
                native,
            },
        }
    }

    pub(crate) fn affects_size(self) -> bool {
        let (reset, size, _) = self.components();
        reset || size.is_some()
    }

    fn components(
        self,
    ) -> (
        bool,
        Option<PreferredWindowSize>,
        Option<NativeWindowGeometry>,
    ) {
        match self {
            Self::Set(size) => (false, Some(size), None),
            Self::SetNative(native) => (false, None, Some(native)),
            Self::Reset => (true, None, None),
            Self::Components {
                reset,
                size,
                native,
            } => (reset, size, native),
        }
    }
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
    fn utility_dialog_sizes_v1_decode_is_lazy_and_upgrades_on_explicit_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let source = br#"{"utility_dialog_sizes":{"version":1,"entries":{"diff":{"state":"preferred","size":{"width":720.0,"height":480.0}}}}}"#;
        std::fs::write(&path, source).unwrap();

        let loaded = Settings::load(path.to_str().unwrap()).unwrap();
        let size = PreferredWindowSize::new(720.0, 480.0).unwrap();
        assert_eq!(
            loaded.utility_dialog_sizes.preferred(key("diff")),
            Some(size)
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            source,
            "reading version-1 preferences does not write an eager upgrade"
        );

        Settings::update(path.to_str().unwrap(), |settings| {
            settings.show_examples = true;
            Ok(())
        })
        .unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["utility_dialog_sizes"]["version"], CURRENT_VERSION);
        assert_eq!(
            saved["utility_dialog_sizes"]["entries"]["diff"]["size"]["width"],
            720.0
        );
    }

    #[test]
    fn utility_dialog_sizes_keep_docked_and_native_geometry_independent() {
        let docked = PreferredWindowSize::new(720.0, 480.0).unwrap();
        let native_size = PreferredWindowSize::new(920.0, 700.0).unwrap();
        let position = SavedWindowPosition::new(-1920.0, -120.0).unwrap();
        let geometry = NativeWindowGeometry::new(position, native_size, 1.5).unwrap();
        let mut sizes = UtilityDialogSizes::default();
        for stable_key in [
            "calculation-pad",
            "markdown-studio",
            "diagram-studio",
            "diagram-viewer",
            "timer",
            "date-time-studio",
        ] {
            assert!(UtilityToolKey::parse(stable_key).is_ok());
        }

        let key = key("calculation-pad");
        assert!(sizes.set(key, docked).unwrap());
        assert!(sizes.set_native_geometry(key, geometry).unwrap());
        assert_eq!(sizes.preferred(key), Some(docked));
        assert_eq!(sizes.native_geometry(key), Some(geometry));

        let encoded = serde_json::to_string(&sizes).unwrap();
        let decoded: UtilityDialogSizes = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.preferred(key), Some(docked));
        assert_eq!(decoded.native_geometry(key), Some(geometry));

        let new_docked = PreferredWindowSize::new(680.0, 440.0).unwrap();
        assert!(sizes.set(key, new_docked).unwrap());
        assert_eq!(sizes.preferred(key), Some(new_docked));
        assert_eq!(sizes.native_geometry(key), Some(geometry));
        assert!(sizes.reset(key).unwrap());
        assert_eq!(sizes.preference(key), UtilitySizePreference::Reset);
        assert_eq!(sizes.native_geometry(key), None);
    }

    #[test]
    fn utility_dialog_sizes_reject_invalid_native_position_and_scale() {
        assert!(SavedWindowPosition::new(f32::NAN, 0.0).is_err());
        assert!(SavedWindowPosition::new(0.0, f32::INFINITY).is_err());
        assert!(SavedWindowPosition::new(MAX_POSITION_COORDINATE + 1.0, 0.0).is_err());
        assert!(PreferredWindowSize::new(0.0, 480.0).is_err());

        let position = SavedWindowPosition::new(-800.0, -100.0).unwrap();
        let size = PreferredWindowSize::new(800.0, 600.0).unwrap();
        for scale in [0.0, f32::NAN, f32::INFINITY, MAX_SCALE_FACTOR + 1.0] {
            assert!(NativeWindowGeometry::new(position, size, scale).is_err());
        }
        let malformed = r#"{"version":2,"entries":{"calculation-pad":{"state":"geometry","native":{"position":{"x":131073.0,"y":0.0},"size":{"width":800.0,"height":600.0},"scale_factor":1.0}}}}"#;
        let sizes: UtilityDialogSizes = serde_json::from_str(malformed).unwrap();
        assert!(!sizes.is_supported());
        assert_eq!(sizes.native_geometry(key("calculation-pad")), None);
    }

    #[test]
    fn utility_size_mutations_merge_independent_components_and_latest_resets() {
        let size = PreferredWindowSize::new(720.0, 480.0).unwrap();
        let geometry = NativeWindowGeometry::new(
            SavedWindowPosition::new(-1920.0, 0.0).unwrap(),
            PreferredWindowSize::new(920.0, 700.0).unwrap(),
            1.25,
        )
        .unwrap();

        assert_eq!(
            UtilitySizeMutation::Set(size).followed_by(UtilitySizeMutation::SetNative(geometry)),
            UtilitySizeMutation::Components {
                reset: false,
                size: Some(size),
                native: Some(geometry),
            }
        );
        assert_eq!(
            UtilitySizeMutation::Set(size).followed_by(UtilitySizeMutation::Reset),
            UtilitySizeMutation::Reset
        );
        assert_eq!(
            UtilitySizeMutation::Reset.followed_by(UtilitySizeMutation::SetNative(geometry)),
            UtilitySizeMutation::Components {
                reset: true,
                size: None,
                native: Some(geometry),
            }
        );
        let reset_with_size =
            UtilitySizeMutation::Reset.followed_by(UtilitySizeMutation::Set(size));
        assert_eq!(
            reset_with_size,
            UtilitySizeMutation::Components {
                reset: true,
                size: Some(size),
                native: None,
            }
        );
        assert_eq!(
            UtilitySizeMutation::SetNative(geometry).followed_by(reset_with_size),
            reset_with_size,
            "a reset-based pending intent retains components written after its reset"
        );
        assert!(UtilitySizeMutation::Reset.affects_size());
        assert!(!UtilitySizeMutation::SetNative(geometry).affects_size());
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
        let geometry = NativeWindowGeometry::new(
            SavedWindowPosition::new(-1280.0, -100.0).unwrap(),
            PreferredWindowSize::new(860.0, 640.0).unwrap(),
            1.5,
        )
        .unwrap();

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
            UtilitySizeMutation::SetNative(geometry),
        )
        .unwrap();
        let restored = Settings::load(path.to_str().unwrap()).unwrap();
        assert_eq!(
            restored.utility_dialog_sizes.native_geometry(key("diff")),
            Some(geometry)
        );

        persist_mutation(
            path.to_str().unwrap(),
            key("diff"),
            UtilitySizeMutation::Reset,
        )
        .unwrap();
        let reset = Settings::load(path.to_str().unwrap()).unwrap();
        assert_eq!(reset.utility_dialog_sizes.preferred(key("diff")), None);
        assert_eq!(
            reset.utility_dialog_sizes.native_geometry(key("diff")),
            None
        );
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
