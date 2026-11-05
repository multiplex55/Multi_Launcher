//! Bounded editable visual document data and its content history.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

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
pub const MAX_VISUAL_ASSETS: usize = 16;
pub const MAX_VISUAL_ASSET_BYTES: usize = 512 * 1024;
pub const MAX_VISUAL_ASSET_TOTAL_BYTES: usize = 512 * 1024;
pub const MAX_VISUAL_ASSET_PIXELS: usize = 4_194_304;
pub const MAX_VISUAL_ASSET_DIMENSION: u32 = 4_096;
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

    pub(super) const fn from_raw(value: u64) -> Self {
        Self(value)
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
pub enum VisualAssetFormat {
    Png,
    Jpeg,
}

/// Immutable, content-addressed local raster data. `payload` is the original
/// bounded PNG/JPEG byte stream; it is never a path, URL, or decoder command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualAsset {
    id: VisualAssetId,
    format: VisualAssetFormat,
    width: u32,
    height: u32,
    #[serde(with = "visual_asset_payload")]
    payload: Arc<[u8]>,
}

impl VisualAsset {
    pub(crate) fn from_verified_parts(
        id: VisualAssetId,
        format: VisualAssetFormat,
        width: u32,
        height: u32,
        payload: Arc<[u8]>,
    ) -> Self {
        Self {
            id,
            format,
            width,
            height,
            payload,
        }
    }

    pub fn id(&self) -> &VisualAssetId {
        &self.id
    }

    pub const fn format(&self) -> VisualAssetFormat {
        self.format
    }

    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn payload(&self) -> &Arc<[u8]> {
        &self.payload
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
    /// Embedded payloads are absent from old v3 documents, preserving their
    /// canonical wire bytes exactly when a scene contains no local assets.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "deserialize_bounded_assets"
    )]
    pub assets: Vec<VisualAsset>,
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
            assets: Vec::new(),
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

impl VisualPresetDraft {
    /// Build a nonrecursive, center-origin preset from an ordered set of
    /// primitive objects. A single uniform scale maps their rotated world
    /// bounds into the existing 100×100 design space while retaining each
    /// local rotation and style.
    pub fn from_primitives(
        name: impl Into<String>,
        objects: &[VisualObject],
    ) -> Result<Self, VisualDocumentError> {
        let name = name.into();
        validate_name(&name)?;
        if objects.is_empty() || objects.len() > MAX_VISUAL_PRESET_COMPONENTS {
            return Err(VisualDocumentError::TooManyPresetComponents {
                max_components: MAX_VISUAL_PRESET_COMPONENTS,
            });
        }
        let mut bounds = None::<(f64, f64, f64, f64)>;
        for object in objects {
            validate_transform(object.transform)?;
            validate_style(&object.style)?;
            let corners = visual_transform_corners(object.transform);
            for (x, y) in corners {
                bounds = Some(bounds.map_or((x, y, x, y), |(min_x, min_y, max_x, max_y)| {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                }));
            }
        }
        let (min_x, min_y, max_x, max_y) = bounds.ok_or(VisualDocumentError::InvalidGeometry)?;
        let width = max_x - min_x;
        let height = max_y - min_y;
        let max_extent = width.max(height);
        if !max_extent.is_finite() || max_extent <= 0.0 {
            return Err(VisualDocumentError::InvalidGeometry);
        }
        let scale = 100.0 / max_extent;
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        let mut components = Vec::with_capacity(objects.len());
        for object in objects {
            let VisualObjectKind::Primitive { shape } = &object.kind else {
                return Err(VisualDocumentError::InvalidState(
                    "only primitive objects can be copied into a reusable preset".into(),
                ));
            };
            let component = VisualPrimitiveComponent {
                shape: *shape,
                transform: VisualTransform {
                    x: (object.transform.x - center_x) * scale,
                    y: (object.transform.y - center_y) * scale,
                    width: object.transform.width * scale,
                    height: object.transform.height * scale,
                    rotation_degrees: object.transform.rotation_degrees,
                },
                style: object.style.clone(),
            };
            validate_component(&component)?;
            components.push(component);
        }
        Ok(Self { name, components })
    }
}

fn visual_transform_corners(transform: VisualTransform) -> [(f64, f64); 4] {
    let (sin, cos) = transform.rotation_degrees.to_radians().sin_cos();
    [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)].map(|(x, y)| {
        let x = x * transform.width;
        let y = y * transform.height;
        (
            transform.x + x * cos - y * sin,
            transform.y + x * sin + y * cos,
        )
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum VisualOperation {
    AddObject(VisualObjectDraft),
    /// Atomically adds (or reuses) embedded bytes and creates their reference.
    AddImageReference {
        asset: VisualAsset,
        layer_id: VisualLayerId,
        transform: VisualTransform,
        style: VisualStyle,
    },
    RemoveObject(VisualObjectId),
    ReplaceObject(VisualObject),
    MoveObject {
        id: VisualObjectId,
        to_index: usize,
    },
    AddLayer {
        name: String,
        to_index: usize,
    },
    RenameLayer {
        id: VisualLayerId,
        name: String,
    },
    RemoveLayer(VisualLayerId),
    MoveLayer {
        id: VisualLayerId,
        to_index: usize,
    },
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
    TooManyAssets { max_assets: usize },
    AssetPayloadTooLarge { max_bytes: usize },
    AssetPayloadsTooLarge { max_bytes: usize },
    AssetPixelsTooLarge { max_pixels: usize },
    TextTooLarge { max_bytes: usize },
    NameInvalid,
    AssetIdInvalid,
    AssetInvalid,
    AssetIdCollision(VisualAssetId),
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
            Self::TooManyAssets { max_assets } => write!(
                formatter,
                "visual document exceeds the {max_assets}-asset limit"
            ),
            Self::AssetPayloadTooLarge { max_bytes } => write!(
                formatter,
                "embedded image exceeds the {max_bytes}-byte limit"
            ),
            Self::AssetPayloadsTooLarge { max_bytes } => write!(
                formatter,
                "embedded images exceed the {max_bytes}-byte total limit"
            ),
            Self::AssetPixelsTooLarge { max_pixels } => write!(
                formatter,
                "embedded images exceed the {max_pixels}-pixel total limit"
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
            Self::AssetInvalid => formatter
                .write_str("embedded image metadata does not match its bounded PNG/JPEG payload"),
            Self::AssetIdCollision(id) => write!(
                formatter,
                "embedded image key {} refers to different content",
                id.as_str()
            ),
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

    /// Construct a fresh, clean visual document from one completely prepared
    /// scene. Conversion uses this to avoid manufacturing edit history or
    /// routing large valid documents through the ordinary GUI operation cap.
    pub(super) fn from_fresh_scene(scene: VisualScene) -> Result<Self, VisualDocumentError> {
        let allocator = VisualAllocator::for_scene(&scene);
        let document = Self {
            saved_baseline: scene.clone(),
            current: scene,
            revision: 0,
            allocator,
            history: VisualHistory::default(),
        };
        document.validate()?;
        Ok(document)
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
        prune_unreferenced_assets(&mut candidate);
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
        VisualOperation::AddImageReference {
            asset,
            layer_id,
            transform,
            style,
        } => {
            validate_visual_asset(asset)?;
            if let Some(existing) = scene.assets.iter().find(|existing| existing.id == asset.id) {
                if existing != asset {
                    return Err(VisualDocumentError::AssetIdCollision(asset.id.clone()));
                }
            } else {
                scene.assets.push(asset.clone());
            }
            let id = allocate_object_id(allocator)?;
            let object = VisualObject {
                id,
                layer_id: *layer_id,
                transform: *transform,
                style: style.clone(),
                kind: VisualObjectKind::ImageReference {
                    asset_id: asset.id.clone(),
                },
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
    if scene.assets.len() > MAX_VISUAL_ASSETS {
        return Err(VisualDocumentError::TooManyAssets {
            max_assets: MAX_VISUAL_ASSETS,
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
    let mut referenced_assets = std::collections::HashSet::new();
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
        if let VisualObjectKind::ImageReference { asset_id } = &object.kind {
            referenced_assets.insert(asset_id.clone());
        }
        if matches!(&object.kind, VisualObjectKind::Connector { .. }) {
            connector_ids.insert(object.id);
        }
    }

    let mut asset_ids = std::collections::HashSet::with_capacity(scene.assets.len());
    let mut total_asset_bytes = 0usize;
    let mut total_asset_pixels = 0usize;
    for asset in &scene.assets {
        validate_visual_asset(asset)?;
        if !asset_ids.insert(asset.id.clone()) {
            return Err(VisualDocumentError::AssetIdCollision(asset.id.clone()));
        }
        if !referenced_assets.contains(&asset.id) {
            return Err(VisualDocumentError::InvalidState(
                "visual scene contains an unreferenced image payload".into(),
            ));
        }
        total_asset_bytes = total_asset_bytes.checked_add(asset.payload.len()).ok_or(
            VisualDocumentError::AssetPayloadsTooLarge {
                max_bytes: MAX_VISUAL_ASSET_TOTAL_BYTES,
            },
        )?;
        total_asset_pixels = total_asset_pixels
            .checked_add(
                usize::try_from(asset.width)
                    .ok()
                    .and_then(|width| {
                        usize::try_from(asset.height)
                            .ok()
                            .and_then(|height| width.checked_mul(height))
                    })
                    .ok_or(VisualDocumentError::AssetPixelsTooLarge {
                        max_pixels: MAX_VISUAL_ASSET_PIXELS,
                    })?,
            )
            .ok_or(VisualDocumentError::AssetPixelsTooLarge {
                max_pixels: MAX_VISUAL_ASSET_PIXELS,
            })?;
    }
    if total_asset_bytes > MAX_VISUAL_ASSET_TOTAL_BYTES {
        return Err(VisualDocumentError::AssetPayloadsTooLarge {
            max_bytes: MAX_VISUAL_ASSET_TOTAL_BYTES,
        });
    }
    if total_asset_pixels > MAX_VISUAL_ASSET_PIXELS {
        return Err(VisualDocumentError::AssetPixelsTooLarge {
            max_pixels: MAX_VISUAL_ASSET_PIXELS,
        });
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

fn validate_visual_asset(asset: &VisualAsset) -> Result<(), VisualDocumentError> {
    validate_asset_id(asset.id.as_str())?;
    if asset.payload.is_empty() || asset.payload.len() > MAX_VISUAL_ASSET_BYTES {
        return Err(VisualDocumentError::AssetPayloadTooLarge {
            max_bytes: MAX_VISUAL_ASSET_BYTES,
        });
    }
    let digest = hex::encode(Sha256::digest(asset.payload.as_ref()));
    if digest != asset.id.as_str() {
        return Err(VisualDocumentError::AssetInvalid);
    }
    let (format, width, height) = crate::image_compare::inspect_local_png_jpeg(
        &asset.payload,
        MAX_VISUAL_ASSET_BYTES,
        MAX_VISUAL_ASSET_PIXELS,
    )
    .map_err(|_| VisualDocumentError::AssetInvalid)?;
    let expected_format = match asset.format {
        VisualAssetFormat::Png => image::ImageFormat::Png,
        VisualAssetFormat::Jpeg => image::ImageFormat::Jpeg,
    };
    if format != expected_format
        || width != asset.width
        || height != asset.height
        || width > MAX_VISUAL_ASSET_DIMENSION
        || height > MAX_VISUAL_ASSET_DIMENSION
    {
        return Err(VisualDocumentError::AssetInvalid);
    }
    Ok(())
}

fn prune_unreferenced_assets(scene: &mut VisualScene) {
    let referenced = scene
        .objects
        .iter()
        .filter_map(|object| match &object.kind {
            VisualObjectKind::ImageReference { asset_id } => Some(asset_id.clone()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    scene.assets.retain(|asset| referenced.contains(&asset.id));
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

fn deserialize_bounded_assets<'de, D>(deserializer: D) -> Result<Vec<VisualAsset>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, _, MAX_VISUAL_ASSETS>(deserializer)
}

mod visual_asset_payload {
    use super::{MAX_VISUAL_ASSET_BYTES, de};
    use base64::Engine as _;
    use serde::de::Visitor;
    use serde::{Deserializer, Serializer};
    use std::fmt;
    use std::sync::Arc;

    pub fn serialize<S>(payload: &Arc<[u8]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(payload.as_ref());
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<[u8]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PayloadVisitor;
        impl<'de> Visitor<'de> for PayloadVisitor {
            type Value = Arc<[u8]>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    formatter,
                    "base64 image bytes no larger than {MAX_VISUAL_ASSET_BYTES} bytes"
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let max_encoded =
                    base64::encoded_len(MAX_VISUAL_ASSET_BYTES, false).unwrap_or(usize::MAX);
                if value.len() > max_encoded {
                    return Err(E::custom(format!(
                        "encoded image exceeds the {MAX_VISUAL_ASSET_BYTES}-byte limit"
                    )));
                }
                let decoded = base64::engine::general_purpose::STANDARD_NO_PAD
                    .decode(value)
                    .map_err(E::custom)?;
                if decoded.len() > MAX_VISUAL_ASSET_BYTES {
                    return Err(E::custom(format!(
                        "image exceeds the {MAX_VISUAL_ASSET_BYTES}-byte limit"
                    )));
                }
                Ok(Arc::from(decoded.into_boxed_slice()))
            }
        }
        deserializer.deserialize_str(PayloadVisitor)
    }
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
        DEFAULT_LAYER_ID, MAX_VISUAL_ASSETS, MAX_VISUAL_DOCUMENT_TITLE_BYTES,
        MAX_VISUAL_HISTORY_BYTES, MAX_VISUAL_SCENE_BYTES, TITLE_OBJECT_ID, VisualAnchor,
        VisualAssetId, VisualColor, VisualConnector, VisualConnectorDirection,
        VisualConnectorEndpoint, VisualConnectorRoute, VisualDiagramDocument, VisualDocumentError,
        VisualHistoryEntry, VisualLayer, VisualLayerId, VisualLineStyle, VisualObject,
        VisualObjectDraft, VisualObjectId, VisualObjectKind, VisualOperation, VisualPresetDraft,
        VisualPrimitiveComponent, VisualPrimitiveShape, VisualScene, VisualShadow, VisualStyle,
        VisualTransform,
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

    fn one_pixel_asset(color: [u8; 4]) -> super::VisualAsset {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba(color));
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        crate::diagram::prepare_visual_asset_bytes(encoded.into_inner(), usize::MAX)
            .unwrap()
            .asset
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
    fn visual_asset_replacement_at_capacity_is_one_atomic_transaction() {
        let mut document = VisualDiagramDocument::new();
        let mut imports = Vec::new();
        for index in 0..MAX_VISUAL_ASSETS {
            let image = image::RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([index as u8, (index as u8).wrapping_add(1), 17, 255]),
            );
            let mut encoded = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image)
                .write_to(&mut encoded, image::ImageFormat::Png)
                .unwrap();
            let prepared =
                crate::diagram::prepare_visual_asset_bytes(encoded.into_inner(), usize::MAX)
                    .unwrap();
            imports.push(prepared.asset);
        }
        let initial_ops = imports
            .iter()
            .cloned()
            .map(|asset| VisualOperation::AddImageReference {
                asset,
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle::default(),
            })
            .collect::<Vec<_>>();
        document.apply_transaction(&initial_ops).unwrap();
        assert_eq!(document.current.assets.len(), MAX_VISUAL_ASSETS);
        let oldest_reference = document
            .current
            .objects
            .iter()
            .find_map(|object| {
                matches!(object.kind, VisualObjectKind::ImageReference { .. }).then_some(object.id)
            })
            .unwrap();

        let replacement_image = image::RgbaImage::from_pixel(1, 1, image::Rgba([240, 12, 31, 255]));
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(replacement_image)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let replacement =
            crate::diagram::prepare_visual_asset_bytes(encoded.into_inner(), usize::MAX)
                .unwrap()
                .asset;
        let outcome = document
            .apply_transaction(&[
                VisualOperation::AddImageReference {
                    asset: replacement.clone(),
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform::default(),
                    style: VisualStyle::default(),
                },
                VisualOperation::RemoveObject(oldest_reference),
            ])
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(document.current.assets.len(), MAX_VISUAL_ASSETS);
        assert!(
            document
                .current
                .assets
                .iter()
                .any(|asset| asset.id() == replacement.id())
        );
    }

    #[test]
    fn visual_asset_base64_wire_rejects_encoded_payload_above_limit_before_decode() {
        let max_encoded = (super::MAX_VISUAL_ASSET_BYTES / 3) * 4
            + match super::MAX_VISUAL_ASSET_BYTES % 3 {
                0 => 0,
                1 => 2,
                _ => 3,
            };
        let encoded = "A".repeat(max_encoded + 1);
        let content_id = "a".repeat(64);
        let json = format!(
            r#"{{"id":"{content_id}","format":"png","width":1,"height":1,"payload":"{encoded}"}}"#
        );
        let error = serde_json::from_str::<super::VisualAsset>(&json).unwrap_err();
        assert!(error.to_string().contains("encoded image exceeds"));
    }

    #[test]
    fn visual_asset_payload_roundtrips_and_prunes_only_after_accepted_reference_removal() {
        let mut document = VisualDiagramDocument::new();
        let asset = one_pixel_asset([23, 57, 91, 255]);
        let outcome = document
            .apply_transaction(&[VisualOperation::AddImageReference {
                asset: asset.clone(),
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    width: 20.0,
                    height: 20.0,
                    ..VisualTransform::default()
                },
                style: VisualStyle::default(),
            }])
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.created_objects.len(), 1);
        assert_eq!(document.scene().assets, vec![asset.clone()]);
        let saved = document.capture_saved_content();
        document.acknowledge_saved_content(&saved);
        assert!(!document.is_dirty());

        let encoded = encode_document(DiagramFileDocumentRef::Visual(&document)).unwrap();
        let DiagramFileDocument::Visual(mut reopened) = decode_document(&encoded).unwrap() else {
            panic!("image document decoded as Mermaid");
        };
        assert_eq!(reopened.scene().assets, vec![asset.clone()]);
        assert!(!reopened.is_dirty());
        assert!(
            reopened
                .apply_transaction(&[VisualOperation::RemoveObject(outcome.created_objects[0])])
                .unwrap()
                .changed
        );
        assert!(reopened.scene().assets.is_empty());
        assert!(reopened.is_dirty());
        assert!(reopened.undo().unwrap());
        assert_eq!(reopened.scene().assets, vec![asset.clone()]);
        assert!(reopened.redo().unwrap());
        assert!(reopened.scene().assets.is_empty());
    }

    #[test]
    fn visual_component_preset_captures_rotated_selection_and_reopens_as_composition() {
        let mut document = VisualDiagramDocument::new();
        let mut first = draft(VisualObjectKind::Primitive {
            shape: VisualPrimitiveShape::Diamond,
        });
        first.transform = VisualTransform {
            x: -100.0,
            y: 25.0,
            width: 44.0,
            height: 26.0,
            rotation_degrees: 35.0,
        };
        first.style.border = VisualColor::rgba(150, 50, 20, 255);
        let mut second = draft(VisualObjectKind::Primitive {
            shape: VisualPrimitiveShape::Circle,
        });
        second.transform = VisualTransform {
            x: 100.0,
            y: 25.0,
            width: 44.0,
            height: 26.0,
            rotation_degrees: 35.0,
        };
        let created = document
            .apply_transaction(&[
                VisualOperation::AddObject(first),
                VisualOperation::AddObject(second),
            ])
            .unwrap();
        let original_objects = document
            .scene()
            .objects
            .iter()
            .filter(|object| created.created_objects.contains(&object.id))
            .cloned()
            .collect::<Vec<_>>();
        let preset =
            VisualPresetDraft::from_primitives("Selected marker", &original_objects).unwrap();
        assert!(preset.components[0].transform.x < 0.0);
        assert!(preset.components[1].transform.x > 0.0);
        assert!(preset.components[0].transform.y.abs() < 1e-9);
        assert!(preset.components[1].transform.y.abs() < 1e-9);
        assert_eq!(preset.components[0].style, original_objects[0].style);

        let preset_id = document
            .apply_transaction(&[VisualOperation::AddPreset(preset)])
            .unwrap()
            .created_presets[0];
        document
            .apply_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    x: 320.0,
                    y: -80.0,
                    width: 100.0,
                    height: 100.0,
                    rotation_degrees: 20.0,
                },
                style: VisualStyle::default(),
                kind: VisualObjectKind::PresetInstance { preset_id },
            })])
            .unwrap();
        assert_eq!(
            document
                .scene()
                .objects
                .iter()
                .filter(|object| created.created_objects.contains(&object.id))
                .cloned()
                .collect::<Vec<_>>(),
            original_objects
        );
        let encoded = encode_document(DiagramFileDocumentRef::Visual(&document)).unwrap();
        let DiagramFileDocument::Visual(reopened) = decode_document(&encoded).unwrap() else {
            panic!("preset document decoded as Mermaid");
        };
        let projection = crate::diagram::project_visual_scene(reopened.scene()).unwrap();
        let mut parts = projection
            .items
            .iter()
            .filter(|item| {
                item.kind
                    == crate::diagram::VisualProjectedKind::PresetPrimitive(
                        VisualPrimitiveShape::Diamond,
                    )
                    || item.kind
                        == crate::diagram::VisualProjectedKind::PresetPrimitive(
                            VisualPrimitiveShape::Circle,
                        )
            })
            .collect::<Vec<_>>();
        parts.sort_by_key(|item| item.component_index);
        assert_eq!(parts.len(), 2);
        assert!((parts[0].transform.x + parts[1].transform.x - 640.0).abs() < 1e-9);
        assert!((parts[0].transform.y + parts[1].transform.y + 160.0).abs() < 1e-9);
        assert_ne!(parts[0].transform.rotation_degrees, 0.0);
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
