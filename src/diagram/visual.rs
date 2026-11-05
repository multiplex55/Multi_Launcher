//! Bounded editable visual document data and its content history.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::VecDeque;
use std::fmt;

pub const MAX_VISUAL_OBJECTS: usize = 1_024;
pub const MAX_VISUAL_LAYERS: usize = 64;
pub const MAX_VISUAL_PRESETS: usize = 128;
pub const MAX_VISUAL_PRESET_COMPONENTS: usize = 64;
pub const MAX_VISUAL_NAME_BYTES: usize = 128;
pub const MAX_VISUAL_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_VISUAL_CONNECTOR_LABEL_BYTES: usize = 4 * 1024;
pub const MAX_VISUAL_DOCUMENT_TITLE_BYTES: usize = 4 * 1024;
pub const MAX_VISUAL_HISTORY_ENTRIES: usize = 64;
pub const MAX_VISUAL_HISTORY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_VISUAL_SCENE_BYTES: usize = MAX_VISUAL_HISTORY_BYTES / 2 - 64 * 1024;
pub const MAX_VISUAL_TRANSACTION_OPERATIONS: usize = 256;
pub const MAX_VISUAL_COORDINATE_ABS: f64 = 1_000_000.0;
pub const MAX_VISUAL_DIMENSION: f64 = 1_000_000.0;
pub const MIN_VISUAL_DIMENSION: f64 = 0.1;
pub const TITLE_OBJECT_ID: VisualObjectId = VisualObjectId(1);
pub const DEFAULT_LAYER_ID: VisualLayerId = VisualLayerId(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VisualObjectId(u64);

impl VisualObjectId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VisualLayerId(u64);

impl VisualLayerId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VisualPresetId(u64);

impl VisualPresetId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// An opaque local asset key. It is not a path, URL, or loading instruction.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct VisualAssetId(String);

impl VisualAssetId {
    pub fn new(value: impl AsRef<str>) -> Result<Self, VisualDocumentError> {
        let value = value.as_ref();
        validate_asset_id(value)?;
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for VisualAssetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserialize_bounded_string::<D, MAX_VISUAL_NAME_BYTES>(deserializer)?;
        validate_asset_id(&value).map_err(de::Error::custom)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualPrimitiveShape {
    Rectangle,
    RoundRect,
    Diamond,
    Circle,
    Ellipse,
    Cylinder,
    Parallelogram,
    Triangle,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualTransform {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_degrees: f64,
}

impl Default for VisualTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 96.0,
            rotation_degrees: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl VisualColor {
    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualLineStyle {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualShadow {
    pub offset_x: f64,
    pub offset_y: f64,
    pub blur: f64,
    pub color: VisualColor,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualStyle {
    pub fill: VisualColor,
    pub border: VisualColor,
    pub border_width: f64,
    pub line_style: VisualLineStyle,
    #[serde(deserialize_with = "deserialize_visual_font")]
    pub font_family: String,
    pub font_size: f64,
    pub opacity: f64,
    pub shadow: Option<VisualShadow>,
}

impl Default for VisualStyle {
    fn default() -> Self {
        Self {
            fill: VisualColor::rgba(245, 247, 250, 255),
            border: VisualColor::rgba(70, 80, 95, 255),
            border_width: 1.5,
            line_style: VisualLineStyle::Solid,
            font_family: "sans-serif".into(),
            font_size: 16.0,
            opacity: 1.0,
            shadow: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisualObjectKind {
    Primitive {
        shape: VisualPrimitiveShape,
    },
    Text {
        #[serde(deserialize_with = "deserialize_visual_text")]
        text: String,
    },
    PresetInstance {
        preset_id: VisualPresetId,
    },
    ImageReference {
        asset_id: VisualAssetId,
    },
    Connector {
        connector: VisualConnector,
    },
}

/// Compass-side anchor in the referenced object's local orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualAnchor {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualConnectorEndpoint {
    pub object_id: VisualObjectId,
    pub anchor: VisualAnchor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualConnectorRoute {
    Straight,
    Elbow,
    Curved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualConnectorDirection {
    None,
    Forward,
    Reverse,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualConnector {
    pub from: VisualConnectorEndpoint,
    pub to: VisualConnectorEndpoint,
    pub route: VisualConnectorRoute,
    pub direction: VisualConnectorDirection,
    #[serde(deserialize_with = "deserialize_connector_label")]
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualObject {
    pub id: VisualObjectId,
    pub layer_id: VisualLayerId,
    pub transform: VisualTransform,
    pub style: VisualStyle,
    pub kind: VisualObjectKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualLayer {
    pub id: VisualLayerId,
    #[serde(deserialize_with = "deserialize_visual_name")]
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualPrimitiveComponent {
    pub shape: VisualPrimitiveShape,
    pub transform: VisualTransform,
    pub style: VisualStyle,
}

/// Preset components are primitives only, so presets cannot recursively call
/// themselves or execute behavior.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualPreset {
    pub id: VisualPresetId,
    #[serde(deserialize_with = "deserialize_visual_name")]
    pub name: String,
    #[serde(deserialize_with = "deserialize_bounded_preset_components")]
    pub components: Vec<VisualPrimitiveComponent>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualScene {
    /// Layers are serialized in bottom-to-top order. Objects are serialized in
    /// back-to-front stack order; neither ordering depends on map iteration.
    #[serde(deserialize_with = "deserialize_bounded_layers")]
    pub layers: Vec<VisualLayer>,
    #[serde(deserialize_with = "deserialize_bounded_objects")]
    pub objects: Vec<VisualObject>,
    #[serde(deserialize_with = "deserialize_bounded_presets")]
    pub presets: Vec<VisualPreset>,
}

impl VisualScene {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            layers: vec![VisualLayer {
                id: DEFAULT_LAYER_ID,
                name: "Layer 1".into(),
            }],
            objects: vec![VisualObject {
                id: TITLE_OBJECT_ID,
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle::default(),
                kind: VisualObjectKind::Text { text: title.into() },
            }],
            presets: Vec::new(),
        }
    }

    pub fn label(&self) -> Option<&str> {
        self.objects
            .iter()
            .find(|object| object.id == TITLE_OBJECT_ID)
            .and_then(|object| match &object.kind {
                VisualObjectKind::Text { text } => Some(text.as_str()),
                _ => None,
            })
    }

    pub fn validate(&self) -> Result<(), VisualDocumentError> {
        validate_scene(self, VisualAllocator::for_scene(self))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VisualAllocator {
    pub next_object_id: u64,
    pub next_layer_id: u64,
    pub next_preset_id: u64,
}

impl VisualAllocator {
    fn initial() -> Self {
        Self {
            next_object_id: 2,
            next_layer_id: 2,
            next_preset_id: 1,
        }
    }

    fn for_scene(scene: &VisualScene) -> Self {
        Self {
            next_object_id: scene
                .objects
                .iter()
                .map(|object| object.id.get())
                .max()
                .unwrap_or(0)
                .saturating_add(1),
            next_layer_id: scene
                .layers
                .iter()
                .map(|layer| layer.id.get())
                .max()
                .unwrap_or(0)
                .saturating_add(1),
            next_preset_id: scene
                .presets
                .iter()
                .map(|preset| preset.id.get())
                .max()
                .unwrap_or(0)
                .saturating_add(1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VisualHistoryEntry {
    pub before: VisualScene,
    pub after: VisualScene,
    #[serde(skip)]
    pub encoded_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct VisualHistory {
    pub undo: VecDeque<VisualHistoryEntry>,
    pub redo: VecDeque<VisualHistoryEntry>,
    pub encoded_bytes: usize,
}

impl VisualHistory {
    fn record(&mut self, entry: VisualHistoryEntry) {
        for redo_entry in self.redo.drain(..) {
            self.encoded_bytes = self.encoded_bytes.saturating_sub(redo_entry.encoded_bytes);
        }
        self.encoded_bytes = self.encoded_bytes.saturating_add(entry.encoded_bytes);
        self.undo.push_back(entry);
        while self.undo.len().saturating_add(self.redo.len()) > MAX_VISUAL_HISTORY_ENTRIES
            || self.encoded_bytes > MAX_VISUAL_HISTORY_BYTES
        {
            let Some(oldest) = self.undo.pop_front() else {
                break;
            };
            self.encoded_bytes = self.encoded_bytes.saturating_sub(oldest.encoded_bytes);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualDiagramDocument {
    pub(super) current: VisualScene,
    pub(super) revision: u64,
    pub(super) allocator: VisualAllocator,
    pub(super) history: VisualHistory,
    pub(super) saved_baseline: VisualScene,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualObjectDraft {
    pub layer_id: VisualLayerId,
    pub transform: VisualTransform,
    pub style: VisualStyle,
    pub kind: VisualObjectKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualPresetDraft {
    pub name: String,
    pub components: Vec<VisualPrimitiveComponent>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VisualOperation {
    AddObject(VisualObjectDraft),
    RemoveObject(VisualObjectId),
    ReplaceObject(VisualObject),
    MoveObject { id: VisualObjectId, to_index: usize },
    AddLayer { name: String, to_index: usize },
    RenameLayer { id: VisualLayerId, name: String },
    RemoveLayer(VisualLayerId),
    MoveLayer { id: VisualLayerId, to_index: usize },
    AddPreset(VisualPresetDraft),
    ReplacePreset(VisualPreset),
    RemovePreset(VisualPresetId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VisualTransactionOutcome {
    pub changed: bool,
    pub created_objects: Vec<VisualObjectId>,
    pub created_layers: Vec<VisualLayerId>,
    pub created_presets: Vec<VisualPresetId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualDocumentError {
    TooManyOperations { max_operations: usize },
    TooManyObjects { max_objects: usize },
    TooManyLayers { max_layers: usize },
    TooManyPresets { max_presets: usize },
    TooManyPresetComponents { max_components: usize },
    TextTooLarge { max_bytes: usize },
    NameInvalid,
    AssetIdInvalid,
    UnknownConnectorEndpoint(VisualObjectId),
    ConnectorEndpointIsConnector(VisualObjectId),
    InvalidConnectorGeometry,
    InvalidId,
    InvalidGeometry,
    InvalidStyle,
    UnknownObject(VisualObjectId),
    UnknownLayer(VisualLayerId),
    UnknownPreset(VisualPresetId),
    DuplicateObjectId(VisualObjectId),
    DuplicateLayerId(VisualLayerId),
    DuplicatePresetId(VisualPresetId),
    TitleObjectRequired,
    TitleObjectMustBeText,
    DefaultLayerRequired,
    DefaultLayerCannotBeRemoved,
    LayerInUse(VisualLayerId),
    PresetInUse(VisualPresetId),
    RevisionExhausted,
    AllocatorExhausted,
    HistoryEntryTooLarge { max_bytes: usize },
    HistoryTooLarge { max_bytes: usize },
    SceneTooLarge { max_bytes: usize },
    HistorySerialization,
    InvalidState(String),
}

impl fmt::Display for VisualDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyOperations { max_operations } => write!(
                formatter,
                "visual edit exceeds the {max_operations}-operation limit"
            ),
            Self::TooManyObjects { max_objects } => write!(
                formatter,
                "visual document exceeds the {max_objects}-object limit"
            ),
            Self::TooManyLayers { max_layers } => write!(
                formatter,
                "visual document exceeds the {max_layers}-layer limit"
            ),
            Self::TooManyPresets { max_presets } => write!(
                formatter,
                "visual document exceeds the {max_presets}-preset limit"
            ),
            Self::TooManyPresetComponents { max_components } => write!(
                formatter,
                "visual preset exceeds the {max_components}-primitive limit"
            ),
            Self::TextTooLarge { max_bytes } => {
                write!(formatter, "visual text exceeds the {max_bytes}-byte limit")
            }
            Self::NameInvalid => {
                formatter.write_str("visual layer, preset, or font name is invalid")
            }
            Self::AssetIdInvalid => {
                formatter.write_str("image reference is not a bounded local asset key")
            }
            Self::UnknownConnectorEndpoint(id) => write!(
                formatter,
                "connector endpoint object {} was not found",
                id.get()
            ),
            Self::ConnectorEndpointIsConnector(id) => write!(
                formatter,
                "connector endpoint object {} is another connector",
                id.get()
            ),
            Self::InvalidConnectorGeometry => formatter.write_str(
                "connector transform is not editable; use its attached endpoint objects",
            ),
            Self::InvalidId => formatter.write_str("visual document contains an invalid stable ID"),
            Self::InvalidGeometry => formatter.write_str(
                "visual object geometry is nonfinite, degenerate, or outside its bounds",
            ),
            Self::InvalidStyle => {
                formatter.write_str("visual object style is invalid or outside its bounds")
            }
            Self::UnknownObject(id) => {
                write!(formatter, "visual object {} was not found", id.get())
            }
            Self::UnknownLayer(id) => write!(formatter, "visual layer {} was not found", id.get()),
            Self::UnknownPreset(id) => {
                write!(formatter, "visual preset {} was not found", id.get())
            }
            Self::DuplicateObjectId(id) => {
                write!(formatter, "visual object ID {} is duplicated", id.get())
            }
            Self::DuplicateLayerId(id) => {
                write!(formatter, "visual layer ID {} is duplicated", id.get())
            }
            Self::DuplicatePresetId(id) => {
                write!(formatter, "visual preset ID {} is duplicated", id.get())
            }
            Self::TitleObjectRequired => {
                formatter.write_str("visual document is missing its title text object")
            }
            Self::TitleObjectMustBeText => {
                formatter.write_str("visual document title object must remain free text")
            }
            Self::DefaultLayerRequired => {
                formatter.write_str("visual document is missing its default layer")
            }
            Self::DefaultLayerCannotBeRemoved => {
                formatter.write_str("the default visual layer cannot be removed")
            }
            Self::LayerInUse(id) => write!(
                formatter,
                "visual layer {} is still used by an object",
                id.get()
            ),
            Self::PresetInUse(id) => write!(
                formatter,
                "visual preset {} is still used by an object",
                id.get()
            ),
            Self::RevisionExhausted => formatter.write_str("visual document revision is exhausted"),
            Self::AllocatorExhausted => {
                formatter.write_str("visual stable ID allocator is exhausted")
            }
            Self::HistoryEntryTooLarge { max_bytes } => write!(
                formatter,
                "visual edit is too large for the {max_bytes}-byte history entry limit"
            ),
            Self::HistoryTooLarge { max_bytes } => write!(
                formatter,
                "visual history exceeds the {max_bytes}-byte limit"
            ),
            Self::SceneTooLarge { max_bytes } => {
                write!(formatter, "visual scene exceeds the {max_bytes}-byte limit")
            }
            Self::HistorySerialization => formatter.write_str("failed to measure visual history"),
            Self::InvalidState(reason) => {
                write!(formatter, "invalid visual document state: {reason}")
            }
        }
    }
}

impl std::error::Error for VisualDocumentError {}

impl VisualDiagramDocument {
    pub fn new() -> Self {
        let current = VisualScene::new("Untitled visual diagram");
        Self {
            saved_baseline: current.clone(),
            current,
            revision: 0,
            allocator: VisualAllocator::initial(),
            history: VisualHistory::default(),
        }
    }

    pub(super) fn from_legacy_title(
        title: impl Into<String>,
        revision: u64,
    ) -> Result<Self, VisualDocumentError> {
        let current = VisualScene::new(title);
        let document = Self {
            saved_baseline: current.clone(),
            current,
            revision,
            allocator: VisualAllocator::initial(),
            history: VisualHistory::default(),
        };
        document.validate()?;
        Ok(document)
    }

    pub(super) fn from_wire(
        current: VisualScene,
        revision: u64,
        allocator: VisualAllocator,
        history_entries: Vec<VisualHistoryEntry>,
        undo_len: usize,
    ) -> Result<Self, VisualDocumentError> {
        if undo_len > history_entries.len() {
            return Err(VisualDocumentError::InvalidState(
                "visual undo split exceeds history length".into(),
            ));
        }
        if history_entries.len() > MAX_VISUAL_HISTORY_ENTRIES {
            return Err(VisualDocumentError::InvalidState(
                "visual history exceeds its entry limit".into(),
            ));
        }

        let mut history = VisualHistory {
            undo: VecDeque::with_capacity(undo_len),
            redo: VecDeque::with_capacity(history_entries.len() - undo_len),
            encoded_bytes: 0,
        };
        for (index, mut entry) in history_entries.into_iter().enumerate() {
            entry.encoded_bytes = visual_history_entry_size(&entry)?;
            history.encoded_bytes = history
                .encoded_bytes
                .checked_add(entry.encoded_bytes)
                .ok_or(VisualDocumentError::HistoryTooLarge {
                    max_bytes: MAX_VISUAL_HISTORY_BYTES,
                })?;
            if history.encoded_bytes > MAX_VISUAL_HISTORY_BYTES {
                return Err(VisualDocumentError::HistoryTooLarge {
                    max_bytes: MAX_VISUAL_HISTORY_BYTES,
                });
            }
            if index < undo_len {
                history.undo.push_back(entry);
            } else {
                history.redo.push_back(entry);
            }
        }
        let document = Self {
            saved_baseline: current.clone(),
            current,
            revision,
            allocator,
            history,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn scene(&self) -> &VisualScene {
        &self.current
    }

    pub fn label(&self) -> Option<&str> {
        self.current.label()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn allocator(&self) -> (u64, u64, u64) {
        (
            self.allocator.next_object_id,
            self.allocator.next_layer_id,
            self.allocator.next_preset_id,
        )
    }

    pub fn is_dirty(&self) -> bool {
        self.current != self.saved_baseline
    }

    pub fn capture_saved_content(&self) -> VisualScene {
        self.current.clone()
    }

    pub fn acknowledge_saved_content(&mut self, saved: &VisualScene) {
        self.saved_baseline = saved.clone();
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.history.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.history.redo.len()
    }

    pub fn replace_label(&mut self, label: impl Into<String>) -> Result<bool, VisualDocumentError> {
        let label = label.into();
        if label.len() > MAX_VISUAL_DOCUMENT_TITLE_BYTES {
            return Err(VisualDocumentError::TextTooLarge {
                max_bytes: MAX_VISUAL_DOCUMENT_TITLE_BYTES,
            });
        }
        let mut title_object = self
            .current
            .objects
            .iter()
            .find(|object| object.id == TITLE_OBJECT_ID)
            .cloned()
            .ok_or(VisualDocumentError::TitleObjectRequired)?;
        let VisualObjectKind::Text { text } = &mut title_object.kind else {
            return Err(VisualDocumentError::TitleObjectMustBeText);
        };
        *text = label;
        Ok(self
            .apply_transaction(&[VisualOperation::ReplaceObject(title_object)])?
            .changed)
    }

    pub fn apply_transaction(
        &mut self,
        operations: &[VisualOperation],
    ) -> Result<VisualTransactionOutcome, VisualDocumentError> {
        if operations.len() > MAX_VISUAL_TRANSACTION_OPERATIONS {
            return Err(VisualDocumentError::TooManyOperations {
                max_operations: MAX_VISUAL_TRANSACTION_OPERATIONS,
            });
        }
        if operations.is_empty() {
            return Ok(VisualTransactionOutcome::default());
        }

        let mut candidate = self.current.clone();
        let mut allocator = self.allocator;
        let mut outcome = VisualTransactionOutcome::default();
        for operation in operations {
            apply_visual_operation(&mut candidate, &mut allocator, operation, &mut outcome)?;
        }
        validate_scene(&candidate, allocator)?;
        if candidate == self.current {
            return Ok(VisualTransactionOutcome::default());
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(VisualDocumentError::RevisionExhausted)?;
        let mut entry = VisualHistoryEntry {
            before: self.current.clone(),
            after: candidate.clone(),
            encoded_bytes: 0,
        };
        entry.encoded_bytes = visual_history_entry_size(&entry)?;
        if entry.encoded_bytes > MAX_VISUAL_HISTORY_BYTES {
            return Err(VisualDocumentError::HistoryEntryTooLarge {
                max_bytes: MAX_VISUAL_HISTORY_BYTES,
            });
        }
        let mut history = self.history.clone();
        history.record(entry);
        if history.encoded_bytes > MAX_VISUAL_HISTORY_BYTES {
            return Err(VisualDocumentError::HistoryTooLarge {
                max_bytes: MAX_VISUAL_HISTORY_BYTES,
            });
        }

        self.current = candidate;
        self.allocator = allocator;
        self.history = history;
        self.revision = next_revision;
        outcome.changed = true;
        Ok(outcome)
    }

    pub fn undo(&mut self) -> Result<bool, VisualDocumentError> {
        let Some(entry) = self.history.undo.back() else {
            return Ok(false);
        };
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(VisualDocumentError::RevisionExhausted)?;
        validate_scene(&entry.before, self.allocator)?;
        let entry = self
            .history
            .undo
            .pop_back()
            .expect("history entry was checked above");
        self.current = entry.before.clone();
        self.history.redo.push_back(entry);
        self.revision = next_revision;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, VisualDocumentError> {
        let Some(entry) = self.history.redo.back() else {
            return Ok(false);
        };
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(VisualDocumentError::RevisionExhausted)?;
        validate_scene(&entry.after, self.allocator)?;
        let entry = self
            .history
            .redo
            .pop_back()
            .expect("history entry was checked above");
        self.current = entry.after.clone();
        self.history.undo.push_back(entry);
        self.revision = next_revision;
        Ok(true)
    }

    pub(super) fn validate(&self) -> Result<(), VisualDocumentError> {
        validate_scene(&self.current, self.allocator)?;
        validate_scene(&self.saved_baseline, self.allocator)?;
        if self
            .history
            .undo
            .len()
            .saturating_add(self.history.redo.len())
            > MAX_VISUAL_HISTORY_ENTRIES
        {
            return Err(VisualDocumentError::InvalidState(
                "visual history exceeds its entry limit".into(),
            ));
        }
        let mut history_bytes = 0usize;
        for entry in self.history.undo.iter().chain(&self.history.redo) {
            validate_scene(&entry.before, self.allocator)?;
            validate_scene(&entry.after, self.allocator)?;
            let encoded_bytes = visual_history_entry_size(entry)?;
            if encoded_bytes != entry.encoded_bytes {
                return Err(VisualDocumentError::InvalidState(
                    "visual history byte accounting disagrees with its encoding".into(),
                ));
            }
            history_bytes = history_bytes.checked_add(encoded_bytes).ok_or(
                VisualDocumentError::HistoryTooLarge {
                    max_bytes: MAX_VISUAL_HISTORY_BYTES,
                },
            )?;
        }
        if history_bytes != self.history.encoded_bytes || history_bytes > MAX_VISUAL_HISTORY_BYTES {
            return Err(VisualDocumentError::HistoryTooLarge {
                max_bytes: MAX_VISUAL_HISTORY_BYTES,
            });
        }
        let minimum_revision = (self.history.undo.len() as u64)
            .checked_add((self.history.redo.len() as u64).saturating_mul(2))
            .ok_or_else(|| {
                VisualDocumentError::InvalidState(
                    "visual revision lower bound is not representable".into(),
                )
            })?;
        if minimum_revision > self.revision {
            return Err(VisualDocumentError::InvalidState(
                "visual revision is lower than its retained history operations".into(),
            ));
        }
        validate_visual_history_chain(&self.current, &self.history)?;
        Ok(())
    }
}

impl Default for VisualDiagramDocument {
    fn default() -> Self {
        Self::new()
    }
}

fn apply_visual_operation(
    scene: &mut VisualScene,
    allocator: &mut VisualAllocator,
    operation: &VisualOperation,
    outcome: &mut VisualTransactionOutcome,
) -> Result<(), VisualDocumentError> {
    match operation {
        VisualOperation::AddObject(draft) => {
            let id = allocate_object_id(allocator)?;
            let object = VisualObject {
                id,
                layer_id: draft.layer_id,
                transform: draft.transform,
                style: draft.style.clone(),
                kind: draft.kind.clone(),
            };
            validate_object(&object)?;
            scene.objects.push(object);
            outcome.created_objects.push(id);
        }
        VisualOperation::RemoveObject(id) => {
            if *id == TITLE_OBJECT_ID {
                return Err(VisualDocumentError::TitleObjectRequired);
            }
            let index = scene
                .objects
                .iter()
                .position(|object| object.id == *id)
                .ok_or(VisualDocumentError::UnknownObject(*id))?;
            scene.objects.remove(index);
            scene.objects.retain(|object| {
                !matches!(
                    &object.kind,
                    VisualObjectKind::Connector { connector }
                        if connector.from.object_id == *id || connector.to.object_id == *id
                )
            });
        }
        VisualOperation::ReplaceObject(replacement) => {
            let index = scene
                .objects
                .iter()
                .position(|object| object.id == replacement.id)
                .ok_or(VisualDocumentError::UnknownObject(replacement.id))?;
            validate_object(replacement)?;
            scene.objects[index] = replacement.clone();
        }
        VisualOperation::MoveObject { id, to_index } => {
            let from_index = scene
                .objects
                .iter()
                .position(|object| object.id == *id)
                .ok_or(VisualDocumentError::UnknownObject(*id))?;
            if *to_index >= scene.objects.len() {
                return Err(VisualDocumentError::InvalidState(
                    "visual object stack index is out of range".into(),
                ));
            }
            let object = scene.objects.remove(from_index);
            scene.objects.insert(*to_index, object);
        }
        VisualOperation::AddLayer { name, to_index } => {
            if scene.layers.len() >= MAX_VISUAL_LAYERS {
                return Err(VisualDocumentError::TooManyLayers {
                    max_layers: MAX_VISUAL_LAYERS,
                });
            }
            validate_name(name)?;
            if *to_index > scene.layers.len() {
                return Err(VisualDocumentError::InvalidState(
                    "visual layer order index is out of range".into(),
                ));
            }
            let id = allocate_layer_id(allocator)?;
            scene.layers.insert(
                *to_index,
                VisualLayer {
                    id,
                    name: name.clone(),
                },
            );
            outcome.created_layers.push(id);
        }
        VisualOperation::RenameLayer { id, name } => {
            validate_name(name)?;
            let layer = scene
                .layers
                .iter_mut()
                .find(|layer| layer.id == *id)
                .ok_or(VisualDocumentError::UnknownLayer(*id))?;
            layer.name = name.clone();
        }
        VisualOperation::RemoveLayer(id) => {
            if *id == DEFAULT_LAYER_ID {
                return Err(VisualDocumentError::DefaultLayerCannotBeRemoved);
            }
            if scene.objects.iter().any(|object| object.layer_id == *id) {
                return Err(VisualDocumentError::LayerInUse(*id));
            }
            let index = scene
                .layers
                .iter()
                .position(|layer| layer.id == *id)
                .ok_or(VisualDocumentError::UnknownLayer(*id))?;
            scene.layers.remove(index);
        }
        VisualOperation::MoveLayer { id, to_index } => {
            let from_index = scene
                .layers
                .iter()
                .position(|layer| layer.id == *id)
                .ok_or(VisualDocumentError::UnknownLayer(*id))?;
            if *to_index >= scene.layers.len() {
                return Err(VisualDocumentError::InvalidState(
                    "visual layer order index is out of range".into(),
                ));
            }
            let layer = scene.layers.remove(from_index);
            scene.layers.insert(*to_index, layer);
        }
        VisualOperation::AddPreset(draft) => {
            if scene.presets.len() >= MAX_VISUAL_PRESETS {
                return Err(VisualDocumentError::TooManyPresets {
                    max_presets: MAX_VISUAL_PRESETS,
                });
            }
            validate_name(&draft.name)?;
            if draft.components.is_empty() || draft.components.len() > MAX_VISUAL_PRESET_COMPONENTS
            {
                return Err(VisualDocumentError::TooManyPresetComponents {
                    max_components: MAX_VISUAL_PRESET_COMPONENTS,
                });
            }
            for component in &draft.components {
                validate_component(component)?;
            }
            let id = allocate_preset_id(allocator)?;
            scene.presets.push(VisualPreset {
                id,
                name: draft.name.clone(),
                components: draft.components.clone(),
            });
            outcome.created_presets.push(id);
        }
        VisualOperation::ReplacePreset(replacement) => {
            validate_preset(replacement)?;
            let preset = scene
                .presets
                .iter_mut()
                .find(|preset| preset.id == replacement.id)
                .ok_or(VisualDocumentError::UnknownPreset(replacement.id))?;
            *preset = replacement.clone();
        }
        VisualOperation::RemovePreset(id) => {
            if scene.objects.iter().any(|object| {
                matches!(object.kind, VisualObjectKind::PresetInstance { preset_id } if preset_id == *id)
            }) {
                return Err(VisualDocumentError::PresetInUse(*id));
            }
            let index = scene
                .presets
                .iter()
                .position(|preset| preset.id == *id)
                .ok_or(VisualDocumentError::UnknownPreset(*id))?;
            scene.presets.remove(index);
        }
    }
    Ok(())
}

fn allocate_object_id(
    allocator: &mut VisualAllocator,
) -> Result<VisualObjectId, VisualDocumentError> {
    let id = VisualObjectId(allocator.next_object_id);
    allocator.next_object_id = allocator
        .next_object_id
        .checked_add(1)
        .ok_or(VisualDocumentError::AllocatorExhausted)?;
    Ok(id)
}

fn allocate_layer_id(
    allocator: &mut VisualAllocator,
) -> Result<VisualLayerId, VisualDocumentError> {
    let id = VisualLayerId(allocator.next_layer_id);
    allocator.next_layer_id = allocator
        .next_layer_id
        .checked_add(1)
        .ok_or(VisualDocumentError::AllocatorExhausted)?;
    Ok(id)
}

fn allocate_preset_id(
    allocator: &mut VisualAllocator,
) -> Result<VisualPresetId, VisualDocumentError> {
    let id = VisualPresetId(allocator.next_preset_id);
    allocator.next_preset_id = allocator
        .next_preset_id
        .checked_add(1)
        .ok_or(VisualDocumentError::AllocatorExhausted)?;
    Ok(id)
}

fn validate_visual_history_chain(
    current: &VisualScene,
    history: &VisualHistory,
) -> Result<(), VisualDocumentError> {
    let all_entries: Vec<&VisualHistoryEntry> = history
        .undo
        .iter()
        .chain(history.redo.iter().rev())
        .collect();
    for entry in &all_entries {
        if entry.before == entry.after {
            return Err(VisualDocumentError::InvalidState(
                "visual history contains a no-op entry".into(),
            ));
        }
    }
    for pair in all_entries.windows(2) {
        if pair[0].after != pair[1].before {
            return Err(VisualDocumentError::InvalidState(
                "visual undo history is not contiguous".into(),
            ));
        }
    }
    if history
        .undo
        .back()
        .is_some_and(|entry| entry.after != *current)
    {
        return Err(VisualDocumentError::InvalidState(
            "visual current scene does not follow undo history".into(),
        ));
    }
    for pair in history.redo.iter().zip(history.redo.iter().skip(1)) {
        if pair.0.before != pair.1.after {
            return Err(VisualDocumentError::InvalidState(
                "visual redo history is not contiguous".into(),
            ));
        }
    }
    if history
        .redo
        .back()
        .is_some_and(|entry| entry.before != *current)
    {
        return Err(VisualDocumentError::InvalidState(
            "visual current scene does not precede redo history".into(),
        ));
    }

    let chronological = all_entries;
    let mut seen_objects = std::collections::HashSet::new();
    let mut seen_layers = std::collections::HashSet::new();
    let mut seen_presets = std::collections::HashSet::new();
    let mut retired_objects = std::collections::HashSet::new();
    let mut retired_layers = std::collections::HashSet::new();
    let mut retired_presets = std::collections::HashSet::new();
    if let Some(first) = chronological.first() {
        collect_scene_ids(
            &first.before,
            &mut seen_objects,
            &mut seen_layers,
            &mut seen_presets,
        );
    }
    for entry in chronological {
        check_scene_id_transition(
            &entry.before,
            &entry.after,
            &mut seen_objects,
            &mut seen_layers,
            &mut seen_presets,
            &mut retired_objects,
            &mut retired_layers,
            &mut retired_presets,
        )?;
    }
    Ok(())
}

fn collect_scene_ids(
    scene: &VisualScene,
    objects: &mut std::collections::HashSet<VisualObjectId>,
    layers: &mut std::collections::HashSet<VisualLayerId>,
    presets: &mut std::collections::HashSet<VisualPresetId>,
) {
    objects.extend(scene.objects.iter().map(|object| object.id));
    layers.extend(scene.layers.iter().map(|layer| layer.id));
    presets.extend(scene.presets.iter().map(|preset| preset.id));
}

#[allow(clippy::too_many_arguments)]
fn check_scene_id_transition(
    before: &VisualScene,
    after: &VisualScene,
    seen_objects: &mut std::collections::HashSet<VisualObjectId>,
    seen_layers: &mut std::collections::HashSet<VisualLayerId>,
    seen_presets: &mut std::collections::HashSet<VisualPresetId>,
    retired_objects: &mut std::collections::HashSet<VisualObjectId>,
    retired_layers: &mut std::collections::HashSet<VisualLayerId>,
    retired_presets: &mut std::collections::HashSet<VisualPresetId>,
) -> Result<(), VisualDocumentError> {
    let before_objects: std::collections::HashSet<_> =
        before.objects.iter().map(|object| object.id).collect();
    let after_objects: std::collections::HashSet<_> =
        after.objects.iter().map(|object| object.id).collect();
    let before_layers: std::collections::HashSet<_> =
        before.layers.iter().map(|layer| layer.id).collect();
    let after_layers: std::collections::HashSet<_> =
        after.layers.iter().map(|layer| layer.id).collect();
    let before_presets: std::collections::HashSet<_> =
        before.presets.iter().map(|preset| preset.id).collect();
    let after_presets: std::collections::HashSet<_> =
        after.presets.iter().map(|preset| preset.id).collect();

    if after_objects
        .difference(&before_objects)
        .any(|id| retired_objects.contains(id) || seen_objects.contains(id))
        || after_layers
            .difference(&before_layers)
            .any(|id| retired_layers.contains(id) || seen_layers.contains(id))
        || after_presets
            .difference(&before_presets)
            .any(|id| retired_presets.contains(id) || seen_presets.contains(id))
    {
        return Err(VisualDocumentError::InvalidState(
            "visual history reintroduces a retired stable ID".into(),
        ));
    }

    retired_objects.extend(before_objects.difference(&after_objects).copied());
    retired_layers.extend(before_layers.difference(&after_layers).copied());
    retired_presets.extend(before_presets.difference(&after_presets).copied());
    seen_objects.extend(after_objects);
    seen_layers.extend(after_layers);
    seen_presets.extend(after_presets);
    Ok(())
}

fn visual_history_entry_size(entry: &VisualHistoryEntry) -> Result<usize, VisualDocumentError> {
    serde_json::to_vec(entry)
        .map(|bytes| bytes.len())
        .map_err(|_| VisualDocumentError::HistorySerialization)
}

fn validate_scene(
    scene: &VisualScene,
    allocator: VisualAllocator,
) -> Result<(), VisualDocumentError> {
    if scene.objects.is_empty() || scene.objects.len() > MAX_VISUAL_OBJECTS {
        return Err(VisualDocumentError::TooManyObjects {
            max_objects: MAX_VISUAL_OBJECTS,
        });
    }
    if scene.layers.is_empty() || scene.layers.len() > MAX_VISUAL_LAYERS {
        return Err(VisualDocumentError::TooManyLayers {
            max_layers: MAX_VISUAL_LAYERS,
        });
    }
    if scene.presets.len() > MAX_VISUAL_PRESETS {
        return Err(VisualDocumentError::TooManyPresets {
            max_presets: MAX_VISUAL_PRESETS,
        });
    }
    if allocator.next_object_id == 0
        || allocator.next_layer_id == 0
        || allocator.next_preset_id == 0
    {
        return Err(VisualDocumentError::InvalidId);
    }

    let mut layer_ids = std::collections::HashSet::with_capacity(scene.layers.len());
    for layer in &scene.layers {
        if layer.id.get() == 0 {
            return Err(VisualDocumentError::InvalidId);
        }
        if !layer_ids.insert(layer.id) {
            return Err(VisualDocumentError::DuplicateLayerId(layer.id));
        }
        validate_name(&layer.name)?;
    }
    if !layer_ids.contains(&DEFAULT_LAYER_ID) {
        return Err(VisualDocumentError::DefaultLayerRequired);
    }

    let mut preset_ids = std::collections::HashSet::with_capacity(scene.presets.len());
    for preset in &scene.presets {
        if preset.id.get() == 0 {
            return Err(VisualDocumentError::InvalidId);
        }
        if !preset_ids.insert(preset.id) {
            return Err(VisualDocumentError::DuplicatePresetId(preset.id));
        }
        validate_preset(preset)?;
    }

    let mut object_ids = std::collections::HashSet::with_capacity(scene.objects.len());
    let mut connector_ids = std::collections::HashSet::new();
    for object in &scene.objects {
        if object.id.get() == 0 {
            return Err(VisualDocumentError::InvalidId);
        }
        if !object_ids.insert(object.id) {
            return Err(VisualDocumentError::DuplicateObjectId(object.id));
        }
        validate_object(object)?;
        if !layer_ids.contains(&object.layer_id) {
            return Err(VisualDocumentError::UnknownLayer(object.layer_id));
        }
        if let VisualObjectKind::PresetInstance { preset_id } = &object.kind
            && !preset_ids.contains(preset_id)
        {
            return Err(VisualDocumentError::UnknownPreset(*preset_id));
        }
        if matches!(&object.kind, VisualObjectKind::Connector { .. }) {
            connector_ids.insert(object.id);
        }
    }

    for object in &scene.objects {
        let VisualObjectKind::Connector { connector } = &object.kind else {
            continue;
        };
        for endpoint in [connector.from, connector.to] {
            if !object_ids.contains(&endpoint.object_id) {
                return Err(VisualDocumentError::UnknownConnectorEndpoint(
                    endpoint.object_id,
                ));
            }
            if connector_ids.contains(&endpoint.object_id) {
                return Err(VisualDocumentError::ConnectorEndpointIsConnector(
                    endpoint.object_id,
                ));
            }
        }
    }

    match scene
        .objects
        .iter()
        .find(|object| object.id == TITLE_OBJECT_ID)
    {
        None => return Err(VisualDocumentError::TitleObjectRequired),
        Some(VisualObject {
            kind: VisualObjectKind::Text { text },
            ..
        }) if text.len() <= MAX_VISUAL_DOCUMENT_TITLE_BYTES => {}
        Some(VisualObject {
            kind: VisualObjectKind::Text { .. },
            ..
        }) => {
            return Err(VisualDocumentError::TextTooLarge {
                max_bytes: MAX_VISUAL_DOCUMENT_TITLE_BYTES,
            });
        }
        Some(_) => return Err(VisualDocumentError::TitleObjectMustBeText),
    }

    let max_object = object_ids.iter().map(|id| id.get()).max().unwrap_or(0);
    let max_layer = layer_ids.iter().map(|id| id.get()).max().unwrap_or(0);
    let max_preset = preset_ids.iter().map(|id| id.get()).max().unwrap_or(0);
    if allocator.next_object_id <= max_object
        || allocator.next_layer_id <= max_layer
        || allocator.next_preset_id <= max_preset
    {
        return Err(VisualDocumentError::InvalidState(
            "visual stable ID allocator is not ahead of the scene".into(),
        ));
    }
    let encoded_size = serde_json::to_vec(scene)
        .map_err(|_| VisualDocumentError::HistorySerialization)?
        .len();
    if encoded_size > MAX_VISUAL_SCENE_BYTES {
        return Err(VisualDocumentError::SceneTooLarge {
            max_bytes: MAX_VISUAL_SCENE_BYTES,
        });
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), VisualDocumentError> {
    if name.is_empty() || name.len() > MAX_VISUAL_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(VisualDocumentError::NameInvalid);
    }
    Ok(())
}

fn validate_asset_id(value: &str) -> Result<(), VisualDocumentError> {
    if value.is_empty()
        || value.len() > MAX_VISUAL_NAME_BYTES
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(VisualDocumentError::AssetIdInvalid);
    }
    Ok(())
}

fn validate_object(object: &VisualObject) -> Result<(), VisualDocumentError> {
    validate_transform(object.transform)?;
    validate_style(&object.style)?;
    match &object.kind {
        VisualObjectKind::Primitive { .. } => {}
        VisualObjectKind::Text { text } => {
            if text.len() > MAX_VISUAL_TEXT_BYTES {
                return Err(VisualDocumentError::TextTooLarge {
                    max_bytes: MAX_VISUAL_TEXT_BYTES,
                });
            }
        }
        VisualObjectKind::PresetInstance { preset_id } if preset_id.get() == 0 => {
            return Err(VisualDocumentError::InvalidId);
        }
        VisualObjectKind::PresetInstance { .. } => {}
        VisualObjectKind::ImageReference { asset_id } => validate_asset_id(asset_id.as_str())?,
        VisualObjectKind::Connector { connector } => {
            if object.transform != VisualTransform::default() {
                return Err(VisualDocumentError::InvalidConnectorGeometry);
            }
            if connector.label.len() > MAX_VISUAL_CONNECTOR_LABEL_BYTES {
                return Err(VisualDocumentError::TextTooLarge {
                    max_bytes: MAX_VISUAL_CONNECTOR_LABEL_BYTES,
                });
            }
        }
    }
    Ok(())
}

fn validate_transform(transform: VisualTransform) -> Result<(), VisualDocumentError> {
    if !transform.x.is_finite()
        || !transform.y.is_finite()
        || !transform.width.is_finite()
        || !transform.height.is_finite()
        || !transform.rotation_degrees.is_finite()
        || transform.x.abs() > MAX_VISUAL_COORDINATE_ABS
        || transform.y.abs() > MAX_VISUAL_COORDINATE_ABS
        || !(MIN_VISUAL_DIMENSION..=MAX_VISUAL_DIMENSION).contains(&transform.width)
        || !(MIN_VISUAL_DIMENSION..=MAX_VISUAL_DIMENSION).contains(&transform.height)
        || transform.rotation_degrees.abs() > 36_000.0
    {
        return Err(VisualDocumentError::InvalidGeometry);
    }
    Ok(())
}

fn validate_style(style: &VisualStyle) -> Result<(), VisualDocumentError> {
    if !style.border_width.is_finite()
        || !(0.0..=128.0).contains(&style.border_width)
        || !style.font_size.is_finite()
        || !(4.0..=256.0).contains(&style.font_size)
        || !style.opacity.is_finite()
        || !(0.0..=1.0).contains(&style.opacity)
        || style.font_family.is_empty()
        || style.font_family.len() > MAX_VISUAL_NAME_BYTES
        || style.font_family.chars().any(char::is_control)
    {
        return Err(VisualDocumentError::InvalidStyle);
    }
    if let Some(shadow) = style.shadow
        && (!shadow.offset_x.is_finite()
            || !shadow.offset_y.is_finite()
            || !shadow.blur.is_finite()
            || shadow.offset_x.abs() > 1_000.0
            || shadow.offset_y.abs() > 1_000.0
            || !(0.0..=128.0).contains(&shadow.blur))
    {
        return Err(VisualDocumentError::InvalidStyle);
    }
    Ok(())
}

fn validate_preset(preset: &VisualPreset) -> Result<(), VisualDocumentError> {
    if preset.id.get() == 0 {
        return Err(VisualDocumentError::InvalidId);
    }
    validate_name(&preset.name)?;
    if preset.components.is_empty() || preset.components.len() > MAX_VISUAL_PRESET_COMPONENTS {
        return Err(VisualDocumentError::TooManyPresetComponents {
            max_components: MAX_VISUAL_PRESET_COMPONENTS,
        });
    }
    for component in &preset.components {
        validate_component(component)?;
    }
    Ok(())
}

fn validate_component(component: &VisualPrimitiveComponent) -> Result<(), VisualDocumentError> {
    validate_transform(component.transform)?;
    validate_style(&component.style)
}

fn deserialize_bounded_string<'de, D, const MAX_BYTES: usize>(
    deserializer: D,
) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoundedStringVisitor<const MAX_BYTES: usize>;
    impl<'de, const MAX_BYTES: usize> Visitor<'de> for BoundedStringVisitor<MAX_BYTES> {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a string no longer than {MAX_BYTES} bytes")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.len() > MAX_BYTES {
                return Err(E::custom(format!(
                    "string exceeds the {MAX_BYTES}-byte limit"
                )));
            }
            Ok(value.to_owned())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.len() > MAX_BYTES {
                return Err(E::custom(format!(
                    "string exceeds the {MAX_BYTES}-byte limit"
                )));
            }
            Ok(value)
        }
    }
    deserializer.deserialize_string(BoundedStringVisitor::<MAX_BYTES>)
}

fn deserialize_visual_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = deserialize_bounded_string::<D, MAX_VISUAL_NAME_BYTES>(deserializer)?;
    validate_name(&value).map_err(de::Error::custom)?;
    Ok(value)
}

fn deserialize_visual_font<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_visual_name(deserializer)
}

fn deserialize_visual_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_string::<D, MAX_VISUAL_TEXT_BYTES>(deserializer)
}

fn deserialize_connector_label<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_string::<D, MAX_VISUAL_CONNECTOR_LABEL_BYTES>(deserializer)
}

fn deserialize_bounded_vec<'de, D, T, const MAX_ITEMS: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct BoundedVecVisitor<T, const MAX_ITEMS: usize>(std::marker::PhantomData<T>);
    impl<'de, T, const MAX_ITEMS: usize> Visitor<'de> for BoundedVecVisitor<T, MAX_ITEMS>
    where
        T: Deserialize<'de>,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a sequence with at most {MAX_ITEMS} items")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let capacity = seq.size_hint().unwrap_or(0).min(MAX_ITEMS);
            let mut values = Vec::with_capacity(capacity);
            while let Some(value) = seq.next_element()? {
                if values.len() == MAX_ITEMS {
                    return Err(de::Error::custom(format!(
                        "sequence exceeds the {MAX_ITEMS}-item limit"
                    )));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(BoundedVecVisitor::<T, MAX_ITEMS>(std::marker::PhantomData))
}

fn deserialize_bounded_layers<'de, D>(deserializer: D) -> Result<Vec<VisualLayer>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, _, MAX_VISUAL_LAYERS>(deserializer)
}

fn deserialize_bounded_objects<'de, D>(deserializer: D) -> Result<Vec<VisualObject>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, _, MAX_VISUAL_OBJECTS>(deserializer)
}

fn deserialize_bounded_presets<'de, D>(deserializer: D) -> Result<Vec<VisualPreset>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, _, MAX_VISUAL_PRESETS>(deserializer)
}

fn deserialize_bounded_preset_components<'de, D>(
    deserializer: D,
) -> Result<Vec<VisualPrimitiveComponent>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, _, MAX_VISUAL_PRESET_COMPONENTS>(deserializer)
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_LAYER_ID, MAX_VISUAL_DOCUMENT_TITLE_BYTES, MAX_VISUAL_HISTORY_BYTES,
        MAX_VISUAL_SCENE_BYTES, TITLE_OBJECT_ID, VisualAnchor, VisualAssetId, VisualColor,
        VisualConnector, VisualConnectorDirection, VisualConnectorEndpoint, VisualConnectorRoute,
        VisualDiagramDocument, VisualDocumentError, VisualHistoryEntry, VisualLayer, VisualLayerId,
        VisualLineStyle, VisualObject, VisualObjectDraft, VisualObjectId, VisualObjectKind,
        VisualOperation, VisualPresetDraft, VisualPrimitiveComponent, VisualPrimitiveShape,
        VisualScene, VisualShadow, VisualStyle, VisualTransform,
    };
    use crate::diagram::codec::{
        DiagramFileDocument, DiagramFileDocumentRef, decode_document, encode_document,
    };

    fn draft(kind: VisualObjectKind) -> VisualObjectDraft {
        VisualObjectDraft {
            layer_id: DEFAULT_LAYER_ID,
            transform: VisualTransform::default(),
            style: VisualStyle::default(),
            kind,
        }
    }

    fn text_draft(text: impl Into<String>) -> VisualObjectDraft {
        draft(VisualObjectKind::Text { text: text.into() })
    }

    fn connector_draft(from: VisualObjectId, to: VisualObjectId) -> VisualObjectDraft {
        draft(VisualObjectKind::Connector {
            connector: VisualConnector {
                from: VisualConnectorEndpoint {
                    object_id: from,
                    anchor: VisualAnchor::East,
                },
                to: VisualConnectorEndpoint {
                    object_id: to,
                    anchor: VisualAnchor::West,
                },
                route: VisualConnectorRoute::Elbow,
                direction: VisualConnectorDirection::Forward,
                label: "depends on".into(),
            },
        })
    }

    fn scene_with_object(id: u64) -> VisualScene {
        let mut scene = VisualScene::new("Untitled visual diagram");
        scene.objects.push(VisualObject {
            id: VisualObjectId(id),
            layer_id: DEFAULT_LAYER_ID,
            transform: VisualTransform::default(),
            style: VisualStyle::default(),
            kind: VisualObjectKind::Primitive {
                shape: VisualPrimitiveShape::Rectangle,
            },
        });
        scene
    }

    #[test]
    fn visual_scene_roundtrips_shapes_styles_layers_order_presets_and_asset_references() {
        let mut document = VisualDiagramDocument::new();
        let layer = document
            .apply_transaction(&[VisualOperation::AddLayer {
                name: "Foreground".into(),
                to_index: 1,
            }])
            .unwrap()
            .created_layers[0];
        let preset = document
            .apply_transaction(&[VisualOperation::AddPreset(VisualPresetDraft {
                name: "Two-part marker".into(),
                components: vec![VisualPrimitiveComponent {
                    shape: VisualPrimitiveShape::Diamond,
                    transform: VisualTransform {
                        x: 0.25,
                        y: -4.5,
                        width: 12.5,
                        height: 8.75,
                        rotation_degrees: 15.5,
                    },
                    style: VisualStyle::default(),
                }],
            })])
            .unwrap()
            .created_presets[0];

        let shapes = [
            VisualPrimitiveShape::Rectangle,
            VisualPrimitiveShape::RoundRect,
            VisualPrimitiveShape::Diamond,
            VisualPrimitiveShape::Circle,
            VisualPrimitiveShape::Ellipse,
            VisualPrimitiveShape::Cylinder,
            VisualPrimitiveShape::Parallelogram,
            VisualPrimitiveShape::Triangle,
        ];
        let mut operations: Vec<_> = shapes
            .into_iter()
            .map(|shape| VisualOperation::AddObject(draft(VisualObjectKind::Primitive { shape })))
            .collect();
        let mut styled = text_draft("fractional text — 東京");
        styled.layer_id = layer;
        styled.transform = VisualTransform {
            x: 3.741_657_386_773_941_3,
            y: -17.125,
            width: 210.75,
            height: 46.5,
            rotation_degrees: 27.125,
        };
        styled.style = VisualStyle {
            fill: VisualColor::rgba(11, 22, 33, 44),
            border: VisualColor::rgba(200, 190, 180, 170),
            border_width: 2.375,
            line_style: VisualLineStyle::Dotted,
            font_family: "Noto Sans JP".into(),
            font_size: 13.375,
            opacity: 0.625,
            shadow: Some(VisualShadow {
                offset_x: 1.25,
                offset_y: -2.75,
                blur: 4.5,
                color: VisualColor::rgba(1, 2, 3, 128),
            }),
        };
        operations.push(VisualOperation::AddObject(styled));
        let mut preset_instance = draft(VisualObjectKind::PresetInstance { preset_id: preset });
        preset_instance.layer_id = layer;
        operations.push(VisualOperation::AddObject(preset_instance));
        let mut image = draft(VisualObjectKind::ImageReference {
            asset_id: VisualAssetId::new("asset-thumb-07").unwrap(),
        });
        image.layer_id = layer;
        operations.push(VisualOperation::AddObject(image));
        operations.push(VisualOperation::AddObject(text_draft("free text")));
        let created = document.apply_transaction(&operations).unwrap();
        assert!(created.changed);
        let move_id = created.created_objects[0];
        let last_index = document.scene().objects.len() - 1;
        document
            .apply_transaction(&[VisualOperation::MoveObject {
                id: move_id,
                to_index: last_index,
            }])
            .unwrap();
        document
            .apply_transaction(&[VisualOperation::MoveLayer {
                id: layer,
                to_index: 0,
            }])
            .unwrap();

        let expected_scene = document.scene().clone();
        let expected_revision = document.revision();
        let expected_allocator = document.allocator();
        let bytes = encode_document(DiagramFileDocumentRef::Visual(&document)).unwrap();
        match decode_document(&bytes).unwrap() {
            DiagramFileDocument::Visual(mut decoded) => {
                assert_eq!(decoded.scene(), &expected_scene);
                assert_eq!(decoded.revision(), expected_revision);
                assert_eq!(decoded.allocator(), expected_allocator);
                assert_eq!(decoded.label(), Some("Untitled visual diagram"));
                assert_eq!(decoded.undo_depth(), document.undo_depth());
                assert_eq!(decoded.redo_depth(), document.redo_depth());
                assert!(!decoded.is_dirty());
                assert!(decoded.undo().unwrap());
                assert!(decoded.redo().unwrap());
                assert_eq!(decoded.scene(), &expected_scene);
            }
            DiagramFileDocument::Mermaid(_) => panic!("visual payload decoded as Mermaid"),
        }
    }

    #[test]
    fn visual_transactions_are_atomic_noop_aware_and_keep_ids_across_undo_and_reopen() {
        let mut document = VisualDiagramDocument::new();
        let initial = document.scene().clone();
        let first = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                },
            ))])
            .unwrap()
            .created_objects[0];
        assert_eq!(first.get(), 2);
        assert!(document.is_dirty());
        let revision = document.revision();
        let depth = document.undo_depth();
        let same = document.scene().objects[1].clone();
        assert!(
            !document
                .apply_transaction(&[VisualOperation::ReplaceObject(same)])
                .unwrap()
                .changed
        );
        assert_eq!(document.revision(), revision);
        assert_eq!(document.undo_depth(), depth);

        let mut invalid = draft(VisualObjectKind::Primitive {
            shape: VisualPrimitiveShape::Ellipse,
        });
        invalid.transform.width = f64::NAN;
        let allocator = document.allocator();
        assert!(matches!(
            document.apply_transaction(&[VisualOperation::AddObject(invalid)]),
            Err(VisualDocumentError::InvalidGeometry)
        ));
        assert_eq!(document.scene().objects.len(), 2);
        assert_eq!(document.allocator(), allocator);
        assert_eq!(document.revision(), revision);

        assert!(document.undo().unwrap());
        assert_eq!(document.scene(), &initial);
        assert!(!document.is_dirty());
        let wire = encode_document(DiagramFileDocumentRef::Visual(&document)).unwrap();
        let DiagramFileDocument::Visual(mut reopened) = decode_document(&wire).unwrap() else {
            panic!("visual payload decoded as Mermaid");
        };
        assert!(reopened.can_redo());
        assert!(
            reopened
                .apply_transaction(&[VisualOperation::AddObject(draft(
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Circle,
                    },
                ))])
                .unwrap()
                .changed
        );
        assert_eq!(reopened.scene().objects[1].id.get(), first.get() + 1);
        assert!(!reopened.can_redo());
        assert_eq!(TITLE_OBJECT_ID.get(), 1);
    }

    #[test]
    fn visual_connector_validates_endpoints_allows_self_loops_and_cascades_delete_in_one_undo() {
        let mut document = VisualDiagramDocument::new();
        let shapes = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                })),
                VisualOperation::AddObject(draft(VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Circle,
                })),
            ])
            .unwrap()
            .created_objects;
        let first = shapes[0];
        let second = shapes[1];
        let connector = document
            .apply_transaction(&[VisualOperation::AddObject(connector_draft(first, second))])
            .unwrap()
            .created_objects[0];
        let before_delete = document.scene().clone();
        let revision = document.revision();

        let unknown_before = document.allocator();
        assert!(matches!(
            document.apply_transaction(&[VisualOperation::AddObject(connector_draft(
                first,
                VisualObjectId(u64::MAX),
            ))]),
            Err(VisualDocumentError::UnknownConnectorEndpoint(id)) if id.get() == u64::MAX
        ));
        assert_eq!(document.revision(), revision);
        assert_eq!(document.allocator(), unknown_before);

        let self_loop = VisualConnector {
            from: VisualConnectorEndpoint {
                object_id: first,
                anchor: VisualAnchor::North,
            },
            to: VisualConnectorEndpoint {
                object_id: first,
                anchor: VisualAnchor::North,
            },
            route: VisualConnectorRoute::Curved,
            direction: VisualConnectorDirection::Both,
            label: String::new(),
        };
        let self_loop_id = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualObjectKind::Connector {
                    connector: self_loop,
                },
            ))])
            .unwrap()
            .created_objects[0];
        let after_self_loop = document.scene().clone();
        assert!(
            document
                .apply_transaction(&[VisualOperation::AddObject(connector_draft(
                    self_loop_id,
                    second,
                ))])
                .is_err()
        );
        assert_eq!(document.scene(), &after_self_loop);

        document
            .apply_transaction(&[VisualOperation::RemoveObject(first)])
            .unwrap();
        assert!(document.scene().objects.iter().all(|object| {
            object.id != first
                && !matches!(
                    &object.kind,
                    VisualObjectKind::Connector { connector }
                        if connector.from.object_id == first || connector.to.object_id == first
                )
        }));
        assert!(document.undo().unwrap());
        assert_eq!(document.scene(), &after_self_loop);
        assert!(document.undo().unwrap());
        assert_eq!(document.scene(), &before_delete);
        assert!(
            document
                .scene()
                .objects
                .iter()
                .any(|object| object.id == connector)
        );
    }

    #[test]
    fn visual_connector_transform_and_label_are_bounded() {
        let mut document = VisualDiagramDocument::new();
        let ids = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                })),
                VisualOperation::AddObject(draft(VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                })),
            ])
            .unwrap()
            .created_objects;
        let id = document
            .apply_transaction(&[VisualOperation::AddObject(connector_draft(ids[0], ids[1]))])
            .unwrap()
            .created_objects[0];
        let mut invalid_transform = document
            .scene()
            .objects
            .iter()
            .find(|object| object.id == id)
            .unwrap()
            .clone();
        invalid_transform.transform.x = 10.0;
        assert_eq!(
            document.apply_transaction(&[VisualOperation::ReplaceObject(invalid_transform)]),
            Err(VisualDocumentError::InvalidConnectorGeometry),
        );

        let mut invalid_label = document
            .scene()
            .objects
            .iter()
            .find(|object| object.id == id)
            .unwrap()
            .clone();
        let VisualObjectKind::Connector { connector } = &mut invalid_label.kind else {
            unreachable!();
        };
        connector.label = "x".repeat(super::MAX_VISUAL_CONNECTOR_LABEL_BYTES + 1);
        assert!(matches!(
            document.apply_transaction(&[VisualOperation::ReplaceObject(invalid_label)]),
            Err(VisualDocumentError::TextTooLarge { max_bytes })
                if max_bytes == super::MAX_VISUAL_CONNECTOR_LABEL_BYTES
        ));
    }

    #[test]
    fn visual_wire_rejects_invalid_scenes_history_and_stable_id_reintroduction() {
        let scene = VisualScene::new("Title");
        let no_op = VisualHistoryEntry {
            before: scene.clone(),
            after: scene.clone(),
            encoded_bytes: 0,
        };
        assert!(
            VisualDiagramDocument::from_wire(
                scene.clone(),
                1,
                super::VisualAllocator::initial(),
                vec![no_op],
                1,
            )
            .is_err()
        );
        assert!(
            VisualDiagramDocument::from_wire(
                scene.clone(),
                1,
                super::VisualAllocator {
                    next_object_id: 3,
                    next_layer_id: 2,
                    next_preset_id: 1,
                },
                vec![VisualHistoryEntry {
                    before: scene.clone(),
                    after: scene_with_object(2),
                    encoded_bytes: 0,
                }],
                0,
            )
            .is_err()
        );

        let with_id = scene_with_object(2);
        let remove_again = scene.clone();
        let history = vec![
            VisualHistoryEntry {
                before: scene.clone(),
                after: with_id.clone(),
                encoded_bytes: 0,
            },
            VisualHistoryEntry {
                before: with_id.clone(),
                after: remove_again.clone(),
                encoded_bytes: 0,
            },
            VisualHistoryEntry {
                before: remove_again.clone(),
                after: with_id.clone(),
                encoded_bytes: 0,
            },
        ];
        assert!(
            VisualDiagramDocument::from_wire(
                with_id,
                3,
                super::VisualAllocator {
                    next_object_id: 3,
                    next_layer_id: 2,
                    next_preset_id: 1,
                },
                history,
                3,
            )
            .is_err()
        );

        assert!(VisualAssetId::new("../outside.png").is_err());
        let mut duplicated = scene;
        duplicated.layers.push(VisualLayer {
            id: VisualLayerId(1),
            name: "Duplicate".into(),
        });
        assert!(duplicated.validate().is_err());

        let mut duplicated_object = VisualScene::new("Title");
        duplicated_object
            .objects
            .push(duplicated_object.objects[0].clone());
        assert!(matches!(
            duplicated_object.validate(),
            Err(VisualDocumentError::DuplicateObjectId(_))
        ));

        let mut unknown_preset = VisualScene::new("Title");
        unknown_preset.objects.push(VisualObject {
            id: VisualObjectId(2),
            layer_id: DEFAULT_LAYER_ID,
            transform: VisualTransform::default(),
            style: VisualStyle::default(),
            kind: VisualObjectKind::PresetInstance {
                preset_id: super::VisualPresetId(99),
            },
        });
        assert!(matches!(
            unknown_preset.validate(),
            Err(VisualDocumentError::UnknownPreset(_))
        ));

        let mut invalid_style = VisualScene::new("Title");
        invalid_style.objects[0].style.opacity = f64::INFINITY;
        assert!(matches!(
            invalid_style.validate(),
            Err(VisualDocumentError::InvalidStyle)
        ));
    }

    #[test]
    fn visual_scene_and_history_budgets_allow_near_limit_edit_undo_and_reject_oversize() {
        let mut document = VisualDiagramDocument::new();
        let long_text = "x".repeat(super::MAX_VISUAL_TEXT_BYTES);
        let operations: Vec<_> = (0..112)
            .map(|_| VisualOperation::AddObject(text_draft(long_text.clone())))
            .collect();
        document.apply_transaction(&operations).unwrap();
        let scene_bytes = serde_json::to_vec(document.scene()).unwrap().len();
        assert!(scene_bytes > MAX_VISUAL_SCENE_BYTES - 300_000);
        assert!(scene_bytes <= MAX_VISUAL_SCENE_BYTES);
        document
            .apply_transaction(&[VisualOperation::AddObject(text_draft("small edit"))])
            .unwrap();
        assert!(document.undo().unwrap());
        assert!(document.scene().objects.len() > 100);
        assert!(document.history.encoded_bytes <= MAX_VISUAL_HISTORY_BYTES);

        let mut too_large = VisualScene::new("Title");
        for index in 0..140 {
            too_large.objects.push(VisualObject {
                id: VisualObjectId(index + 2),
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle::default(),
                kind: VisualObjectKind::Text {
                    text: long_text.clone(),
                },
            });
        }
        assert!(matches!(
            VisualDiagramDocument::from_wire(
                too_large,
                0,
                super::VisualAllocator {
                    next_object_id: 142,
                    next_layer_id: 2,
                    next_preset_id: 1,
                },
                Vec::new(),
                0,
            ),
            Err(VisualDocumentError::SceneTooLarge { .. })
        ));
        assert!(MAX_VISUAL_DOCUMENT_TITLE_BYTES < super::MAX_VISUAL_TEXT_BYTES);
    }
}
