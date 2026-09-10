use super::{
    image_search::{AlphaPolicy, ReturnPoint},
    screen::{ScreenRect, SearchRegion},
};
use crate::mkmacro::variables::{MkPoint, MkValue, MkValueSource, MkValueType};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

// Schema 14 adds persisted recorder settings and the expanded physical-key model.
// Existing schema-13 documents retain their macro content and receive defaults.
pub const SCHEMA_VERSION: u32 = 14;
fn schema() -> u32 {
    SCHEMA_VERSION
}
fn yes() -> bool {
    true
}
fn one() -> u32 {
    1
}

/// A portable reference to one PNG directly inside the shared `mkmacro_assets`
/// directory. Filesystem roots are owned by the store, never by this value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MkImageRef(String);

impl MkImageRef {
    pub fn new(filename: impl Into<String>) -> Result<Self, String> {
        let filename = filename.into();
        validate_image_filename(&filename)?;
        Ok(Self(filename))
    }

    /// Constructs a reference without checking the filesystem. Migration uses
    /// this for deterministic missing references so normal validation can report them.
    pub fn from_filename(filename: impl Into<String>) -> Self {
        Self(filename.into())
    }

    pub fn filename(&self) -> &str {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.filename()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn is_valid_filename(&self) -> bool {
        validate_image_filename(&self.0).is_ok()
    }
}

fn validate_image_filename(filename: &str) -> Result<(), String> {
    if filename.is_empty() {
        return Err("image filename must not be empty".into());
    }
    if filename == "." || filename == ".." {
        return Err("image filename must not be '.' or '..'".into());
    }
    if filename.contains('/') || filename.contains('\\') {
        return Err("image filename must be a direct child of mkmacro_assets".into());
    }
    if filename.starts_with('/')
        || filename.starts_with("\\\\")
        || (filename.as_bytes().get(1) == Some(&b':')
            && filename
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic))
    {
        return Err("image filename must not be an absolute or drive/UNC path".into());
    }
    if !filename.to_ascii_lowercase().ends_with(".png") {
        return Err("image filename must end with .png".into());
    }
    if filename
        .chars()
        .any(|c| c.is_control() || "<>:\"|?*".contains(c))
    {
        return Err("image filename contains invalid characters".into());
    }
    if filename.ends_with('.') || filename.ends_with(' ') {
        return Err("image filename must not end with a dot or space".into());
    }
    let stem = filename[..filename.len() - 4].trim_end_matches(['.', ' ']);
    if stem.is_empty() {
        return Err("image filename must have a non-empty basename".into());
    }
    let reserved = stem.split('.').next().unwrap_or(stem).to_ascii_uppercase();
    if matches!(
        reserved.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Err("image filename uses a reserved device name".into());
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMacroDocument {
    #[serde(default = "schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub macros: Vec<MkMacro>,
    #[serde(default)]
    pub folders: Vec<MkMacroFolder>,
    /// Document-wide authoring controls shared by every macro.
    #[serde(default)]
    pub settings: MkMacroSettings,
}
impl Default for MkMacroDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            macros: vec![],
            folders: vec![],
            settings: MkMacroSettings::default(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMacroFolder {
    pub id: u64,
    pub name: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMacroSettings {
    pub record_toggle_hotkey: MkHotkey,
    #[serde(default)]
    pub recorder: MkRecorderSettings,
}
impl Default for MkMacroSettings {
    fn default() -> Self {
        Self {
            record_toggle_hotkey: MkHotkey {
                key: MkKey::Function(9),
                modifiers: vec![],
            },
            recorder: MkRecorderSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementMode {
    Off,
    ClicksOnly,
    SampledMovement,
    DetailedMovement,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkRecorderSettings {
    pub record_keyboard: bool,
    pub record_mouse_buttons: bool,
    pub record_mouse_wheel: bool,
    pub movement_mode: MovementMode,
    pub movement_distance_px: i32,
    pub movement_interval_ms: u64,
    pub click_max_ms: u64,
    pub click_distance_px: i32,
    pub multi_click_ms: u64,
    pub record_injected_input: bool,
    pub record_window_context: bool,
    pub minimum_idle_delay_ms: u64,
    pub delay_rounding_ms: u64,
    pub key_tap_max_ms: u64,
    pub text_run_gap_ms: u64,
    pub smart_keyboard_cleanup: bool,
    pub smart_mouse_cleanup: bool,
    pub smart_window_cleanup: bool,
    pub smart_repeated_click_cleanup: bool,
    pub detect_application_launches: bool,
    pub inspect_clicked_controls: bool,
    pub capture_text_paste_for_freeze_suggestion: bool,
    pub repeated_click_minimum: u32,
    pub repeated_click_interval_tolerance_ms: u64,
    #[serde(default)]
    pub pause_resume_hotkey: Option<MkHotkey>,
    #[serde(default)]
    pub marker_hotkey: Option<MkHotkey>,
}

impl Default for MkRecorderSettings {
    fn default() -> Self {
        Self {
            record_keyboard: true,
            record_mouse_buttons: true,
            record_mouse_wheel: true,
            movement_mode: MovementMode::SampledMovement,
            movement_distance_px: 16,
            movement_interval_ms: 80,
            click_max_ms: 500,
            click_distance_px: 4,
            multi_click_ms: 500,
            record_injected_input: false,
            record_window_context: true,
            minimum_idle_delay_ms: 100,
            delay_rounding_ms: 10,
            key_tap_max_ms: 500,
            text_run_gap_ms: 750,
            smart_keyboard_cleanup: true,
            smart_mouse_cleanup: true,
            smart_window_cleanup: true,
            smart_repeated_click_cleanup: true,
            detect_application_launches: true,
            inspect_clicked_controls: true,
            capture_text_paste_for_freeze_suggestion: true,
            repeated_click_minimum: 3,
            repeated_click_interval_tolerance_ms: 100,
            pause_resume_hotkey: None,
            marker_hotkey: None,
        }
    }
}

impl MkRecorderSettings {
    pub fn clamp(&mut self) {
        self.movement_distance_px = self.movement_distance_px.clamp(1, 500);
        self.movement_interval_ms = self.movement_interval_ms.clamp(1, 5_000);
        self.click_max_ms = self.click_max_ms.clamp(1, 10_000);
        self.click_distance_px = self.click_distance_px.clamp(0, 100);
        self.multi_click_ms = self.multi_click_ms.clamp(1, 5_000);
        self.minimum_idle_delay_ms = self.minimum_idle_delay_ms.min(60_000);
        self.delay_rounding_ms = self.delay_rounding_ms.clamp(1, 10_000);
        self.key_tap_max_ms = self.key_tap_max_ms.clamp(1, 10_000);
        self.text_run_gap_ms = self.text_run_gap_ms.clamp(1, 60_000);
        self.repeated_click_minimum = self.repeated_click_minimum.clamp(2, 100);
        self.repeated_click_interval_tolerance_ms =
            self.repeated_click_interval_tolerance_ms.min(10_000);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMacro {
    #[serde(default)]
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub hotkey: Option<MkHotkey>,
    #[serde(default)]
    pub hotkey_scope: MkHotkeyScope,
    /// Folder membership is authoring metadata and is intentionally ignored by runtime execution.
    #[serde(default)]
    pub folder_id: Option<u64>,
    #[serde(default)]
    pub playback: MkPlayback,
    #[serde(default)]
    pub signature: MkMacroSignature,
    #[serde(default)]
    pub steps: Vec<MkStep>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkStep {
    #[serde(default)]
    pub id: u64,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Persisted authoring/debug metadata. It has no execution effect unless
    /// the runtime explicitly uses Debug mode.
    #[serde(default)]
    pub breakpoint: bool,
    #[serde(default = "one")]
    pub repeat: u32,
    #[serde(default)]
    pub delay_after_ms: u64,
    #[serde(default)]
    pub on_error: MkErrorPolicy,
    #[serde(default)]
    pub metadata: MkStepMetadata,
    pub action: MkAction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkStepMetadata {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub accent: MkStepAccent,
    #[serde(default)]
    pub bookmarked: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkStepAccent {
    #[default]
    Default,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Gray,
}

/// Parameters and outputs share a namespace within their owning macro.
/// Invalid IDs remain deserializable so authoring diagnostics can repair them
/// explicitly without guessing which existing Call/Return bindings were intended.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct MkSignatureId(pub u64);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MkMacroSignature {
    #[serde(default)]
    pub parameters: Vec<MkMacroParameter>,
    #[serde(default)]
    pub outputs: Vec<MkMacroOutput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMacroParameter {
    #[serde(default)]
    pub id: MkSignatureId,
    pub name: String,
    pub value_type: MkValueType,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default_value: Option<MkValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMacroOutput {
    #[serde(default)]
    pub id: MkSignatureId,
    pub name: String,
    pub value_type: MkValueType,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkCallArgumentBinding {
    pub parameter_id: MkSignatureId,
    pub source: MkValueSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkCallOutputBinding {
    pub output_id: MkSignatureId,
    pub caller_variable: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MkCallMacroPayload {
    pub macro_id: u64,
    #[serde(default)]
    pub arguments: Vec<MkCallArgumentBinding>,
    #[serde(default)]
    pub outputs: Vec<MkCallOutputBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkReturnValueBinding {
    pub output_id: MkSignatureId,
    pub source: MkValueSource,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MkReturnPayload {
    #[serde(default)]
    pub outputs: Vec<MkReturnValueBinding>,
}

impl MkMacroDocument {
    /// Allocate for authoring only. Reserve dangling references as well as live
    /// definitions: deleting a definition must never retarget an existing binding.
    /// The caller inserts the new definition before requesting another ID.
    pub fn next_signature_id(&self, macro_id: u64) -> Option<MkSignatureId> {
        let mut owners = self.macros.iter().filter(|m| m.id == macro_id);
        let owner = owners.next()?;
        if macro_id == 0 || owners.next().is_some() {
            return None;
        }
        let mut used: HashSet<u64> = owner
            .signature
            .parameters
            .iter()
            .map(|p| p.id.0)
            .chain(owner.signature.outputs.iter().map(|o| o.id.0))
            .collect();
        for m in &self.macros {
            for step in &m.steps {
                match &step.action {
                    MkAction::CallMacro(call) if call.macro_id == macro_id => {
                        used.extend(call.arguments.iter().map(|b| b.parameter_id.0));
                        used.extend(call.outputs.iter().map(|b| b.output_id.0));
                    }
                    MkAction::Return(ret) if m.id == macro_id => {
                        used.extend(ret.outputs.iter().map(|b| b.output_id.0));
                    }
                    _ => {}
                }
            }
        }
        let mut next = used
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .unwrap_or(1);
        next_unused_id(&used, &mut next).map(MkSignatureId)
    }
}

/// Returns the next unused positive ID and advances the cursor. Wrap after the
/// high-water mark overflows, checking the complete namespace before reuse.
pub(crate) fn next_unused_id(used: &HashSet<u64>, next: &mut u64) -> Option<u64> {
    let start = (*next).max(1);
    let mut candidate = start;
    loop {
        if !used.contains(&candidate) {
            *next = candidate.checked_add(1).unwrap_or(1);
            return Some(candidate);
        }
        candidate = candidate.checked_add(1).unwrap_or(1);
        if candidate == start {
            return None;
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkHotkey {
    pub key: MkKey,
    #[serde(default)]
    pub modifiers: Vec<MkKey>,
}
/// A hotkey-dispatch constraint, not a macro execution permission.
///
/// Only the hotkey service checks the foreground window against this scope.
/// Compilation, direct runtime/toolbar Run commands, and macro invocation do
/// not require a matching foreground window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MkHotkeyScope {
    #[default]
    AnyWindow,
    ActiveWindow(MkWindowMatcher),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkPlayback {
    #[serde(default = "one")]
    pub speed_percent: u32,
    #[serde(default)]
    pub random_delay_ms: u64,
    #[serde(default)]
    pub random_offset_px: u32,
}
impl Default for MkPlayback {
    fn default() -> Self {
        Self {
            speed_percent: 100,
            random_delay_ms: 0,
            random_offset_px: 0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum MkErrorPolicy {
    #[default]
    Stop,
    Continue,
    Retry(MkRetry),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkRetry {
    pub attempts: u32,
    pub delay_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkKey {
    Character(String),
    Enter,
    Tab,
    Escape,
    Space,
    Backspace,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    PauseBreak,
    Control,
    LeftControl,
    RightControl,
    Alt,
    LeftAlt,
    RightAlt,
    Shift,
    LeftShift,
    RightShift,
    Meta,
    LeftMeta,
    RightMeta,
    Function(u8),
    Numpad(u8),
    NumpadMultiply,
    NumpadAdd,
    NumpadSeparator,
    NumpadSubtract,
    NumpadDecimal,
    NumpadDivide,
    OemSemicolon,
    OemEquals,
    OemComma,
    OemMinus,
    OemPeriod,
    OemSlash,
    OemBacktick,
    OemLeftBracket,
    OemBackslash,
    OemRightBracket,
    OemQuote,
    Oem102,
    BrowserBack,
    BrowserForward,
    BrowserRefresh,
    BrowserStop,
    BrowserSearch,
    BrowserFavorites,
    BrowserHome,
    VolumeMute,
    VolumeDown,
    VolumeUp,
    MediaNext,
    MediaPrevious,
    MediaStop,
    MediaPlayPause,
    LaunchMail,
    LaunchMediaSelect,
    LaunchApp1,
    LaunchApp2,
    RawVirtualKey {
        vk: u16,
        scan_code: u16,
        extended: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkTextMode {
    Type,
    Paste,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkMouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MkCoordinateTarget {
    CurrentPosition,
    Screen {
        point: MkPoint,
    },
    ActiveWindow {
        point: MkPoint,
    },
    WindowClient {
        matcher: MkWindowMatcher,
        point: MkPoint,
    },
    Variable {
        name: String,
    },
    Image {
        image: MkImageRef,
        offset: MkPoint,
    },
    /// Result produced by a particular Find Pixel Color action.
    Pixel {
        search_id: u64,
        #[serde(default)]
        offset: MkPoint,
    },
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkWindowMatcher {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_regex: Option<String>,
    #[serde(default)]
    pub process: Option<String>,
    #[serde(default)]
    pub class: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkUiSelector {
    #[serde(default)]
    pub automation_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub control_type: Option<MkUiControlType>,
    #[serde(default)]
    pub class_name: Option<String>,
    #[serde(default)]
    pub framework_id: Option<String>,
    /// Nearest ancestor first. Each entry must contain at least one identity field.
    #[serde(default)]
    pub ancestor_path: Vec<MkUiSelectorPart>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkUiControlType {
    Button,
    Edit,
    CheckBox,
    RadioButton,
    ComboBox,
    ListItem,
    TabItem,
    MenuItem,
    TreeItem,
    Text,
    Custom,
    Other(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MkUiSelectorPart {
    #[serde(default)]
    pub automation_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub class_name: Option<String>,
    #[serde(default)]
    pub control_type: Option<MkUiControlType>,
    #[serde(default)]
    pub framework_id: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkUiPattern {
    Invoke,
    Value,
    Toggle,
    SelectionItem,
    Focus,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkWaitOptions {
    pub timeout_ms: u64,
    pub poll_interval_ms: u64,
}
impl MkWaitOptions {
    /// Returns the finite timeout, or `None` when this wait has no timeout
    /// deadline. `None` still polls until success or an external abort.
    pub fn timeout_duration(&self) -> Option<Duration> {
        timeout_duration(self.timeout_ms)
    }
}

/// Canonical conversion used by every wait payload: zero means no deadline.
fn timeout_duration(timeout_ms: u64) -> Option<Duration> {
    (timeout_ms != 0).then(|| Duration::from_millis(timeout_ms))
}
impl Default for MkWaitOptions {
    fn default() -> Self {
        Self {
            timeout_ms: 1_000,
            poll_interval_ms: 50,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkCompareOp {
    Eq,
    NotEq,
    Less,
    LessOrEq,
    Greater,
    GreaterOrEq,
    Contains,
    StartsWith,
    EndsWith,
    Regex,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MkCondition {
    Variable {
        name: String,
        op: MkCompareOp,
        value: MkValue,
    },
    WindowExists {
        matcher: MkWindowMatcher,
    },
    WindowActive {
        matcher: MkWindowMatcher,
    },
    ImageSearch {
        search: MkImageSearchCondition,
        found: bool,
    },
    OcrTextSearch {
        search: MkOcrSearchCondition,
        found: bool,
    },
    PreviousImageResult {
        image: Option<MkImageRef>,
        found: bool,
    },
    PixelResult {
        target: MkCoordinateTarget,
        color: String,
        tolerance: u8,
    },
    All {
        conditions: Vec<MkCondition>,
    },
    Any {
        conditions: Vec<MkCondition>,
    },
    Not {
        condition: Box<MkCondition>,
    },
}

impl MkCondition {
    /// Returns whether this condition tree performs OCR when evaluated.
    pub(crate) fn contains_ocr(&self) -> bool {
        match self {
            Self::OcrTextSearch { .. } => true,
            Self::All { conditions } | Self::Any { conditions } => {
                conditions.iter().any(Self::contains_ocr)
            }
            Self::Not { condition } => condition.contains_ocr(),
            _ => false,
        }
    }
}

/// A single, immediate image search used by a condition.  Action polling and
/// output policy deliberately live in [`MkImagePayload`], not here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkImageSearchCondition {
    #[serde(default)]
    pub image: MkImageRef,
    #[serde(default)]
    pub region: SearchRegion,
    #[serde(default)]
    pub tolerance: u8,
    #[serde(default)]
    pub alpha: AlphaPolicy,
    #[serde(default)]
    pub return_point: ReturnPoint,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MkOcrLanguage {
    #[default]
    Auto,
    LanguageTag(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkOcrMatchMode {
    #[default]
    Contains,
    WholeWordPhrase,
    Regex,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum MkOcrOccurrence {
    #[default]
    First,
    Nth(u32),
}

impl MkOcrOccurrence {
    /// Zero-based selected index. An invalid persisted `Nth(0)` deliberately
    /// selects nothing until document validation asks the author to repair it.
    pub fn selected_index(self) -> Option<usize> {
        match self {
            Self::First => Some(0),
            Self::Nth(n) => usize::try_from(n.checked_sub(1)?).ok(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MkOcrSearchSpec {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub region: SearchRegion,
    #[serde(default)]
    pub language: MkOcrLanguage,
    #[serde(default)]
    pub match_mode: MkOcrMatchMode,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub occurrence: MkOcrOccurrence,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MkOcrSearchCondition {
    #[serde(flatten)]
    pub search: MkOcrSearchSpec,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkOcrOutputs {
    #[serde(default)]
    pub found: Option<String>,
    #[serde(default)]
    pub matched_text: Option<String>,
    #[serde(default)]
    pub point: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
    #[serde(default)]
    pub match_count: Option<String>,
}

impl MkOcrOutputs {
    pub fn normalize(&mut self) {
        for value in [
            &mut self.found,
            &mut self.matched_text,
            &mut self.point,
            &mut self.x,
            &mut self.y,
            &mut self.match_count,
        ] {
            *value = value.take().and_then(|name| {
                let name = name.trim();
                (!name.is_empty()).then(|| name.to_owned())
            });
        }
    }
}

fn default_ocr_wait() -> MkWaitOptions {
    MkWaitOptions {
        timeout_ms: 5_000,
        poll_interval_ms: 250,
    }
}

fn default_ocr_button() -> MkMouseButton {
    MkMouseButton::Left
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkOcrFindPayload {
    #[serde(default)]
    pub search: MkOcrSearchSpec,
    #[serde(default = "default_ocr_wait")]
    pub wait: MkWaitOptions,
    #[serde(default)]
    pub not_found_policy: MkImageNotFoundPolicy,
    #[serde(default)]
    pub outputs: MkOcrOutputs,
}

impl Default for MkOcrFindPayload {
    fn default() -> Self {
        Self {
            search: MkOcrSearchSpec::default(),
            wait: default_ocr_wait(),
            not_found_policy: MkImageNotFoundPolicy::Continue,
            outputs: MkOcrOutputs::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkOcrClickPayload {
    #[serde(default)]
    pub search: MkOcrSearchSpec,
    #[serde(default = "default_ocr_wait")]
    pub wait: MkWaitOptions,
    #[serde(default = "legacy_image_not_found_policy")]
    pub not_found_policy: MkImageNotFoundPolicy,
    #[serde(default = "default_ocr_button")]
    pub button: MkMouseButton,
    #[serde(default = "one")]
    pub clicks: u32,
    #[serde(default)]
    pub x_offset: i32,
    #[serde(default)]
    pub y_offset: i32,
}

impl Default for MkOcrClickPayload {
    fn default() -> Self {
        Self {
            search: MkOcrSearchSpec::default(),
            wait: default_ocr_wait(),
            not_found_policy: MkImageNotFoundPolicy::Fail,
            button: MkMouseButton::Left,
            clicks: 1,
            x_offset: 0,
            y_offset: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MkOcrReadPayload {
    #[serde(default)]
    pub region: SearchRegion,
    #[serde(default)]
    pub language: MkOcrLanguage,
    #[serde(default)]
    pub output_variable: String,
}

impl MkImageSearchCondition {
    pub fn as_payload(&self) -> MkImagePayload {
        MkImagePayload {
            image: self.image.clone(),
            wait: MkWaitOptions {
                timeout_ms: 0,
                poll_interval_ms: 1,
            },
            region: self.region.clone(),
            tolerance: self.tolerance,
            alpha: self.alpha,
            return_point: self.return_point,
            not_found_policy: MkImageNotFoundPolicy::Continue,
            outputs: MkImageOutputs::default(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkTextPayload {
    pub text: String,
    pub mode: MkTextMode,
}
fn notification_title() -> String {
    "Macro Notification".into()
}
fn notification_description() -> String {
    "Macro completed".into()
}
fn notification_sound() -> String {
    "ReminderStart.wav".into()
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkNotificationKind {
    #[default]
    Information,
    Success,
    Warning,
    Error,
}
impl MkNotificationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Information => "Information",
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Error => "Error",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Information => "ℹ",
            Self::Success => "✓",
            Self::Warning => "⚠",
            Self::Error => "✕",
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkNotificationDuration {
    #[default]
    Short,
    Long,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkNotifyPayload {
    #[serde(default = "notification_title")]
    pub title: String,
    #[serde(default = "notification_description")]
    pub description: String,
    #[serde(default)]
    pub kind: MkNotificationKind,
    #[serde(default)]
    pub duration: MkNotificationDuration,
    #[serde(default = "yes")]
    pub show_symbol: bool,
}
impl Default for MkNotifyPayload {
    fn default() -> Self {
        Self {
            title: notification_title(),
            description: notification_description(),
            kind: MkNotificationKind::Information,
            duration: MkNotificationDuration::Short,
            show_symbol: true,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkPlaySoundPayload {
    #[serde(default = "notification_sound")]
    pub sound: String,
}
impl Default for MkPlaySoundPayload {
    fn default() -> Self {
        Self {
            sound: notification_sound(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMousePayload {
    pub target: MkCoordinateTarget,
    pub button: MkMouseButton,
    #[serde(default = "one")]
    pub clicks: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkClickWithinRegionPayload {
    pub rect: ScreenRect,
    pub button: MkMouseButton,
    #[serde(default = "one")]
    pub clicks: u32,
    #[serde(default)]
    pub edge_padding_px: u32,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MkDelayMode {
    #[default]
    #[serde(rename = "fixed")]
    Fixed,
    #[serde(rename = "random_range")]
    RandomRange,
}
/// Maximum delay duration accepted by macro authoring and execution validation.
pub const MAX_DELAY_MS: u64 = 86_400_000;

/// Delay fields are persisted together for schema compatibility, but only the
/// fields selected by `mode` are semantically active. Validation bounds
/// `fixed_ms` in fixed mode and both range endpoints in random-range mode;
/// inactive fields are intentionally ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkDelayPayload {
    #[serde(default, rename = "mode")]
    pub mode: MkDelayMode,
    #[serde(default = "default_delay_ms", rename = "fixed_ms")]
    pub fixed_ms: u64,
    #[serde(default, rename = "minimum_ms")]
    pub minimum_ms: u64,
    #[serde(default, rename = "maximum_ms")]
    pub maximum_ms: u64,
}
fn default_delay_ms() -> u64 {
    1_000
}
impl Default for MkDelayPayload {
    fn default() -> Self {
        Self {
            mode: MkDelayMode::Fixed,
            fixed_ms: default_delay_ms(),
            minimum_ms: 0,
            maximum_ms: 0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMouseMovePayload {
    pub target: MkCoordinateTarget,
    #[serde(default)]
    pub duration_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMouseDragPayload {
    pub from: MkCoordinateTarget,
    pub to: MkCoordinateTarget,
    pub button: MkMouseButton,
    #[serde(default)]
    pub duration_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkProcessPayload {
    pub program: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub wait: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkWindowPayload {
    pub matcher: MkWindowMatcher,
    #[serde(default)]
    pub wait: Option<MkWaitOptions>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkWindowState {
    Minimize,
    Maximize,
    Restore,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkWindowMoveResizePayload {
    pub matcher: MkWindowMatcher,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkImageNotFoundPolicy {
    Continue,
    Fail,
}
impl Default for MkImageNotFoundPolicy {
    /// Authoring default. Persisted legacy data deliberately uses
    /// `legacy_image_not_found_policy` instead.
    fn default() -> Self {
        Self::Continue
    }
}
fn legacy_image_not_found_policy() -> MkImageNotFoundPolicy {
    MkImageNotFoundPolicy::Fail
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkImageOutputs {
    #[serde(default)]
    pub found: Option<String>,
    #[serde(default)]
    pub point: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
}
impl MkImageOutputs {
    pub fn normalize(&mut self) {
        for value in [&mut self.found, &mut self.point, &mut self.x, &mut self.y] {
            *value = value.take().and_then(|name| {
                let name = name.trim();
                (!name.is_empty()).then(|| name.to_owned())
            });
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkImagePayload {
    #[serde(default)]
    pub image: MkImageRef,
    pub wait: MkWaitOptions,
    #[serde(default)]
    pub region: SearchRegion,
    #[serde(default)]
    pub tolerance: u8,
    #[serde(default)]
    pub alpha: AlphaPolicy,
    #[serde(default)]
    pub return_point: ReturnPoint,
    /// Missing in historical documents meant that absence failed the action.
    #[serde(default = "legacy_image_not_found_policy")]
    pub not_found_policy: MkImageNotFoundPolicy,
    #[serde(default)]
    pub outputs: MkImageOutputs,
}
/// A first-class, asset-independent pixel search. Tolerance is the maximum
/// absolute difference allowed independently for each RGB channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkPixelSearchPayload {
    /// Stable identity used to isolate coordinate results from other searches.
    #[serde(default)]
    pub search_id: u64,
    pub color: String,
    #[serde(default)]
    pub tolerance: u8,
    #[serde(default)]
    pub region: SearchRegion,
    #[serde(default)]
    pub wait: MkWaitOptions,
    #[serde(default)]
    pub not_found_policy: MkImageNotFoundPolicy,
    #[serde(default)]
    pub outputs: MkImageOutputs,
}
/// Where a captured screenshot is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkScreenshotDestination {
    File,
    Clipboard,
    Both,
}

impl MkScreenshotDestination {
    pub fn produces_file(self) -> bool {
        matches!(self, Self::File | Self::Both)
    }
    pub fn produces_clipboard(self) -> bool {
        matches!(self, Self::Clipboard | Self::Both)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkScreenshotFormat {
    Png,
    Jpeg,
    Bmp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkFileCollisionPolicy {
    Error,
    Overwrite,
    Unique,
}

/// Persisted Capture Screenshot action. `path` is an interpolation template and
/// is deliberately optional so clipboard-only actions do not retain a dormant
/// file destination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkScreenshotPayload {
    #[serde(default)]
    pub region: SearchRegion,
    pub destination: MkScreenshotDestination,
    #[serde(default)]
    pub path: Option<String>,
    pub format: MkScreenshotFormat,
    pub collision: MkFileCollisionPolicy,
    #[serde(default)]
    pub path_output: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkUiPayload {
    pub window: MkWindowMatcher,
    pub selector: MkUiSelector,
    #[serde(default)]
    pub wait: Option<MkWaitOptions>,
}
fn prompt_title() -> String {
    "Input Required".into()
}
fn prompt_variable() -> String {
    "input".into()
}
/// Persisted configuration for an interactive macro input step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkPromptInputPayload {
    #[serde(default = "prompt_title")]
    pub title: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub default_value: String,
    #[serde(default = "prompt_variable")]
    pub variable: String,
    #[serde(default)]
    pub copy_to_clipboard: bool,
}
impl Default for MkPromptInputPayload {
    fn default() -> Self {
        Self {
            title: prompt_title(),
            prompt: String::new(),
            default_value: String::new(),
            variable: prompt_variable(),
            copy_to_clipboard: false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkVirtualDesktopAction {
    Create,
    SwitchLeft,
    SwitchRight,
    CloseCurrent,
    GoTo { desktop: u32 },
}
/// The wheel axis used by a mouse-scroll action.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkMouseScrollAxis {
    /// The default preserves compatibility with actions serialized before axes existed.
    #[default]
    Vertical,
    Horizontal,
}
/// Waits until a stable percentage of pixels differs from the frame captured
/// when the action starts. `change_threshold_percent` is in user-facing percent
/// units (5.0 means five percent); a pixel is changed when any RGBA channel
/// differs by more than `per_pixel_tolerance`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitForVisualChange {
    pub region: SearchRegion,
    pub timeout_ms: u64,
    pub poll_interval_ms: u64,
    pub change_threshold_percent: f64,
    #[serde(default)]
    pub per_pixel_tolerance: Option<u8>,
    #[serde(default)]
    pub consecutive_changed_frames: Option<u32>,
}
impl Default for WaitForVisualChange {
    fn default() -> Self {
        Self {
            region: SearchRegion::Desktop,
            timeout_ms: 10_000,
            poll_interval_ms: 100,
            change_threshold_percent: 5.0,
            per_pixel_tolerance: Some(8),
            consecutive_changed_frames: Some(2),
        }
    }
}
impl WaitForVisualChange {
    /// Returns the finite timeout, or `None` for an indefinitely polling wait.
    pub fn timeout_duration(&self) -> Option<Duration> {
        timeout_duration(self.timeout_ms)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MkAction {
    CallMacro(MkCallMacroPayload),
    Return(MkReturnPayload),
    KeyDown(MkKey),
    KeyUp(MkKey),
    KeyPress(MkKey),
    Hotkey(Vec<MkKey>),
    Text(MkTextPayload),
    Notify(MkNotifyPayload),
    PlaySound(MkPlaySoundPayload),
    MouseMove(MkMouseMovePayload),
    MouseDrag(MkMouseDragPayload),
    MouseClick(MkMousePayload),
    ClickWithinRegion(MkClickWithinRegionPayload),
    MouseDown(MkMouseButton),
    MouseUp(MkMouseButton),
    MouseScroll {
        #[serde(default)]
        axis: MkMouseScrollAxis,
        i32_delta: i32,
    },
    Delay(MkDelayPayload),
    Process(MkProcessPayload),
    LauncherCommand(MkLauncherCommandPayload),
    WindowActivate(MkWindowPayload),
    WindowClose(MkWindowMatcher),
    WindowWait(MkWindowPayload),
    WindowMoveResize(MkWindowMoveResizePayload),
    WindowState {
        matcher: MkWindowMatcher,
        state: MkWindowState,
    },
    VirtualDesktop(MkVirtualDesktopAction),
    /// The single polling action. Window/image/pixel wait rows are editor conveniences
    /// and are normalized to this behavior by the executor.
    WaitUntil {
        condition: MkCondition,
        wait: MkWaitOptions,
    },
    SetVariable {
        name: String,
        value: MkValue,
    },
    UnsetVariable {
        name: String,
    },
    PromptInput(MkPromptInputPayload),
    If(MkCondition),
    Else,
    EndIf,
    RepeatStart {
        count: u32,
    },
    RepeatEnd,
    WhileStart {
        condition: MkCondition,
    },
    WhileEnd,
    Break,
    Continue,
    ImageFind(MkImagePayload),
    ImageClick(MkImagePayload),
    OcrFindText(MkOcrFindPayload),
    OcrClickText(MkOcrClickPayload),
    OcrReadText(MkOcrReadPayload),
    FindPixel(MkPixelSearchPayload),
    CaptureScreenshot(MkScreenshotPayload),
    WaitForVisualChange(WaitForVisualChange),
    PixelCheck {
        target: MkCoordinateTarget,
        color: String,
        tolerance: u8,
    },
    UiInvoke(MkUiPayload),
    UiSetValue {
        target: MkUiPayload,
        value: String,
    },
    UiReadValue {
        target: MkUiPayload,
        variable: String,
    },
    UiToggle(MkUiPayload),
    UiSelect(MkUiPayload),
    UiFocus(MkUiPayload),
    UiWait(MkUiPayload),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MkLauncherCommandPayload {
    /// Text submitted to the Launcher's normal search and command pipeline.
    #[serde(default)]
    pub query: String,
    /// A resolved action retained only while reading macros authored by older versions.
    ///
    /// New and editor-created actions always set this to `None`. `Some` is written
    /// exclusively by the v7-to-v8 migration, and editing `query` permanently
    /// clears it so the action thereafter uses normal query behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_resolved_action: Option<crate::actions::Action>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MkBlockKind {
    If,
    Repeat,
    While,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MkBlockMarker {
    Open(MkBlockKind),
    Else,
    Close(MkBlockKind),
}
impl MkAction {
    pub fn block_marker(&self) -> Option<MkBlockMarker> {
        match self {
            Self::If(_) => Some(MkBlockMarker::Open(MkBlockKind::If)),
            Self::Else => Some(MkBlockMarker::Else),
            Self::EndIf => Some(MkBlockMarker::Close(MkBlockKind::If)),
            Self::RepeatStart { .. } => Some(MkBlockMarker::Open(MkBlockKind::Repeat)),
            Self::RepeatEnd => Some(MkBlockMarker::Close(MkBlockKind::Repeat)),
            Self::WhileStart { .. } => Some(MkBlockMarker::Open(MkBlockKind::While)),
            Self::WhileEnd => Some(MkBlockMarker::Close(MkBlockKind::While)),
            _ => None,
        }
    }
    /// Returns whether this action is a block boundary used by the editor.
    /// Loop-control instructions are structural at runtime, but are not block markers.
    pub fn is_block_marker(&self) -> bool {
        self.block_marker().is_some()
    }
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            Self::If(_)
                | Self::Else
                | Self::EndIf
                | Self::RepeatStart { .. }
                | Self::RepeatEnd
                | Self::WhileStart { .. }
                | Self::WhileEnd
                | Self::Break
                | Self::Continue
        )
    }
    pub fn can_be_disabled(&self) -> bool {
        !matches!(
            self,
            Self::Else | Self::EndIf | Self::RepeatEnd | Self::WhileEnd
        )
    }
}

#[cfg(test)]
mod reusable_model_tests {
    use super::*;

    fn document() -> MkMacroDocument {
        serde_json::from_value(serde_json::json!({
            "macros": [{"id": 1, "name": "callee", "signature": {
                "parameters": [{"id": 1, "name": "input", "value_type": "number", "default_value": {"type": "number", "value": 2.5}}],
                "outputs": [{"id": 2, "name": "result", "value_type": "point"}]
            }, "steps": [{"id": 1, "breakpoint": true, "metadata": {
                "label": "Finish", "comment": "Return the result", "accent": "purple", "bookmarked": true
            }, "action": {"type": "return", "data": {"outputs": [{"output_id": 2, "source": {
                "type": "literal", "data": {"type": "point", "value": {"x": -5, "y": 9}}
            }}]}}}]}, {"id": 2, "name": "caller", "steps": [{"id": 1, "action": {
                "type": "call_macro", "data": {"macro_id": 1,
                    "arguments": [{"parameter_id": 1, "source": {"type": "variable", "data": {"name": "mouse.x"}}}],
                    "outputs": [{"output_id": 2, "caller_variable": "point"}]
                }
            }}]}]
        })).unwrap()
    }

    #[test]
    fn metadata_signature_and_reusable_actions_round_trip() {
        let original = document();
        let json = serde_json::to_value(&original).unwrap();
        assert_eq!(json["schema_version"], SCHEMA_VERSION);
        assert_eq!(json["macros"][0]["signature"]["parameters"][0]["id"], 1);
        assert_eq!(
            json["macros"][0]["steps"][0]["metadata"]["accent"],
            "purple"
        );
        assert_eq!(
            serde_json::from_value::<MkMacroDocument>(json).unwrap(),
            original
        );
        assert_eq!(original.macros[1].signature, MkMacroSignature::default());
        assert_eq!(
            original.macros[1].steps[0].metadata,
            MkStepMetadata::default()
        );
        for accent in [
            MkStepAccent::Default,
            MkStepAccent::Red,
            MkStepAccent::Orange,
            MkStepAccent::Yellow,
            MkStepAccent::Green,
            MkStepAccent::Blue,
            MkStepAccent::Purple,
            MkStepAccent::Gray,
        ] {
            assert_eq!(
                serde_json::from_value::<MkStepAccent>(serde_json::to_value(accent).unwrap())
                    .unwrap(),
                accent
            );
        }
        assert!(serde_json::from_str::<MkStepAccent>("\"#ff0000\"").is_err());
    }

    #[test]
    fn signature_allocation_reserves_dangling_bindings_without_repair() {
        let mut doc = document();
        doc.macros[0].signature = MkMacroSignature::default();
        let before = doc.clone();
        assert!(!super::super::store::repair_ids(&mut doc));
        assert_eq!(doc, before);
        assert_eq!(doc.next_signature_id(1), Some(MkSignatureId(3)));
        assert_eq!(doc.next_signature_id(2), Some(MkSignatureId(1)));
        assert_eq!(doc.next_signature_id(99), None);

        // A Return site with no corresponding caller still reserves its dangling ID.
        if let MkAction::Return(ret) = &mut doc.macros[0].steps[0].action {
            ret.outputs[0].output_id = MkSignatureId(u64::MAX);
        }
        assert_eq!(doc.next_signature_id(1), Some(MkSignatureId(3)));
    }

    #[test]
    fn malformed_signature_identity_is_retained_and_diagnosed() {
        let mut doc = document();
        doc.macros[0].signature.outputs[0].id = MkSignatureId(1);
        doc.macros[0].signature.parameters.push(MkMacroParameter {
            id: MkSignatureId(0),
            name: "broken".into(),
            value_type: MkValueType::String,
            description: String::new(),
            default_value: None,
        });
        let original = doc.clone();
        assert!(!super::super::store::repair_ids(&mut doc));
        assert_eq!(doc, original);
        let diagnostics = super::super::validation::validate_document(&doc, None);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "invalid_signature_id")
                .count(),
            2
        );
        assert!(!super::super::validation::can_run(&diagnostics));
    }
}

#[cfg(test)]
mod launcher_command_payload_tests {
    use super::*;
    use crate::actions::Action;

    #[test]
    fn normal_query_round_trips_with_variables_and_has_canonical_shape() {
        let action = MkAction::LauncherCommand(MkLauncherCommandPayload {
            query: "note list ${project name}".into(),
            legacy_resolved_action: None,
        });

        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(
            json,
            r#"{"type":"launcher_command","data":{"query":"note list ${project name}"}}"#
        );
        assert!(json.contains("\"query\""));
        assert!(!json.contains("legacy_resolved_action"));
        assert!(!json.contains("\"command\""));
        assert!(!json.contains("\"args\""));
        assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
    }

    #[test]
    fn legacy_resolved_action_round_trips_without_losing_arguments() {
        let action = MkAction::LauncherCommand(MkLauncherCommandPayload {
            query: "legacy tool".into(),
            legacy_resolved_action: Some(Action {
                label: "Legacy Tool".into(),
                desc: "Imported from schema 7".into(),
                action: "tool.exe".into(),
                args: Some("--profile \"work notes\"".into()),
            }),
        });

        let json = serde_json::to_string(&action).unwrap();
        let decoded: MkAction = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, action);
        let MkAction::LauncherCommand(payload) = decoded else {
            unreachable!()
        };
        assert_eq!(
            payload.legacy_resolved_action.unwrap().args.as_deref(),
            Some("--profile \"work notes\"")
        );
    }

    #[test]
    fn default_payload_cannot_populate_legacy_compatibility_state() {
        let payload = MkLauncherCommandPayload::default();
        assert!(payload.query.is_empty());
        assert!(payload.legacy_resolved_action.is_none());
    }

    #[test]
    fn current_document_round_trips_as_current_schema_with_query_payload() {
        let document = MkMacroDocument {
            macros: vec![MkMacro {
                signature: Default::default(),
                id: 1,
                name: "launcher".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: MkPlayback::default(),
                steps: vec![MkStep {
                    metadata: Default::default(),
                    id: 2,
                    enabled: true,
                    breakpoint: false,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: MkErrorPolicy::Stop,
                    action: MkAction::LauncherCommand(MkLauncherCommandPayload {
                        query: "note list".into(),
                        legacy_resolved_action: None,
                    }),
                }],
            }],
            ..MkMacroDocument::default()
        };

        let json = serde_json::to_string(&document).unwrap();
        assert!(json.contains(&format!("\"schema_version\":{SCHEMA_VERSION}")));
        assert!(json.contains(r#""data":{"query":"note list"}"#));
        assert_eq!(
            serde_json::from_str::<MkMacroDocument>(&json).unwrap(),
            document
        );
    }
}
#[cfg(test)]
mod image_payload_tests {
    use super::*;
    #[test]
    fn missing_image_search_fields_default_and_current_round_trips() {
        let current =
            r#"{"image":"login_button.png","wait":{"timeout_ms":10,"poll_interval_ms":2}}"#;
        let p: MkImagePayload = serde_json::from_str(current).unwrap();
        assert_eq!(p.region, SearchRegion::Desktop);
        assert_eq!(p.tolerance, 0);
        assert_eq!(p.alpha, AlphaPolicy::Compare);
        assert_eq!(p.return_point, ReturnPoint::Center);
        assert_eq!(p.not_found_policy, MkImageNotFoundPolicy::Fail);
        assert_eq!(p.outputs, MkImageOutputs::default());
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<MkImagePayload>(&json).unwrap(), p);
    }
    #[test]
    fn image_policy_and_outputs_have_stable_json_and_round_trip() {
        let mut p: MkImagePayload = serde_json::from_str(
            r#"{"image":"login_button.png","wait":{"timeout_ms":10,"poll_interval_ms":2}}"#,
        )
        .unwrap();
        p.not_found_policy = MkImageNotFoundPolicy::Continue;
        p.outputs = MkImageOutputs {
            found: Some("found_out".into()),
            point: Some("point_out".into()),
            x: Some("x_out".into()),
            y: Some("y_out".into()),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains(r#""not_found_policy":"continue""#));
        assert_eq!(serde_json::from_str::<MkImagePayload>(&json).unwrap(), p);
        p.not_found_policy = MkImageNotFoundPolicy::Fail;
        assert!(
            serde_json::to_string(&p)
                .unwrap()
                .contains(r#""not_found_policy":"fail""#)
        );
    }

    #[test]
    fn schema_eleven_image_consumers_serialize_filename_references_only() {
        let image = MkImageRef::from_filename("login_button.png");
        let values = [
            serde_json::to_value(MkAction::ImageFind(MkImagePayload {
                image: image.clone(),
                wait: MkWaitOptions::default(),
                region: SearchRegion::Desktop,
                tolerance: 0,
                alpha: AlphaPolicy::Compare,
                return_point: ReturnPoint::Center,
                not_found_policy: MkImageNotFoundPolicy::Fail,
                outputs: MkImageOutputs::default(),
            }))
            .unwrap(),
            serde_json::to_value(MkCondition::PreviousImageResult {
                image: Some(image.clone()),
                found: true,
            })
            .unwrap(),
            serde_json::to_value(MkCoordinateTarget::Image {
                image,
                offset: MkPoint { x: 2, y: -1 },
            })
            .unwrap(),
        ];
        let text = serde_json::to_string(&values).unwrap();
        assert!(text.contains(r#""image":"login_button.png""#));
        assert!(!text.contains("asset_id"));
        assert!(!text.contains("image_assets"));
    }
    #[test]
    fn matched_window_coordinate_has_stable_tag_and_round_trips() {
        let target = MkCoordinateTarget::WindowClient {
            matcher: MkWindowMatcher {
                process: Some("app.exe".into()),
                title: Some("Editor".into()),
                ..Default::default()
            },
            point: MkPoint { x: -4, y: 12 },
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains(r#""kind":"window_client""#));
        assert_eq!(
            serde_json::from_str::<MkCoordinateTarget>(&json).unwrap(),
            target
        );
        let old: MkCoordinateTarget =
            serde_json::from_str(r#"{"kind":"active_window","point":{"x":1,"y":2}}"#).unwrap();
        assert_eq!(
            old,
            MkCoordinateTarget::ActiveWindow {
                point: MkPoint { x: 1, y: 2 }
            }
        );
    }

    #[test]
    fn current_position_coordinate_has_stable_tag_and_round_trips() {
        let target = MkCoordinateTarget::CurrentPosition;
        assert_eq!(
            serde_json::to_string(&target).unwrap(),
            r#"{"kind":"current_position"}"#
        );
        assert_eq!(
            serde_json::from_str::<MkCoordinateTarget>(r#"{"kind":"current_position"}"#).unwrap(),
            target
        );
    }
    #[test]
    fn new_window_actions_have_stable_tags_and_round_trip() {
        let matcher = MkWindowMatcher {
            title: Some("Editor".into()),
            ..Default::default()
        };
        let actions = [
            MkAction::WindowMoveResize(MkWindowMoveResizePayload {
                matcher: matcher.clone(),
                x: Some(-1920),
                y: Some(-20),
                width: None,
                height: None,
            }),
            MkAction::WindowMoveResize(MkWindowMoveResizePayload {
                matcher: matcher.clone(),
                x: None,
                y: None,
                width: Some(1200),
                height: Some(800),
            }),
            MkAction::WindowMoveResize(MkWindowMoveResizePayload {
                matcher: matcher.clone(),
                x: Some(-1),
                y: Some(2),
                width: Some(3),
                height: Some(4),
            }),
            MkAction::WindowState {
                matcher: matcher.clone(),
                state: MkWindowState::Minimize,
            },
            MkAction::WindowState {
                matcher: matcher.clone(),
                state: MkWindowState::Maximize,
            },
            MkAction::WindowState {
                matcher,
                state: MkWindowState::Restore,
            },
        ];
        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            assert!(
                json.contains(if matches!(action, MkAction::WindowMoveResize(_)) {
                    r#""type":"window_move_resize""#
                } else {
                    r#""type":"window_state""#
                })
            );
            assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
        }
        assert_eq!(
            serde_json::to_string(&MkWindowState::Minimize).unwrap(),
            r#""minimize""#
        );
        assert_eq!(
            serde_json::to_string(&MkWindowState::Maximize).unwrap(),
            r#""maximize""#
        );
        assert_eq!(
            serde_json::to_string(&MkWindowState::Restore).unwrap(),
            r#""restore""#
        );
    }

    #[test]
    fn virtual_desktop_actions_have_stable_tags_and_round_trip() {
        let cases = [
            (MkVirtualDesktopAction::Create, "create"),
            (MkVirtualDesktopAction::SwitchLeft, "switch_left"),
            (MkVirtualDesktopAction::SwitchRight, "switch_right"),
            (MkVirtualDesktopAction::CloseCurrent, "close_current"),
        ];
        for (operation, serialized) in cases {
            let action = MkAction::VirtualDesktop(operation);
            let json = serde_json::to_string(&action).unwrap();
            assert_eq!(
                json,
                format!(r#"{{"type":"virtual_desktop","data":"{serialized}"}}"#)
            );
            assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
        }
        // A representative pre-feature action document remains compatible.
        let old = r#"{"schema_version":4,"macros":[],"settings":{"record_toggle_hotkey":{"key":{"function":9},"modifiers":[]}}}"#;
        assert!(serde_json::from_str::<MkMacroDocument>(old).is_ok());
    }
}

#[cfg(test)]
mod screenshot_region_serialization_tests {
    use super::*;

    #[test]
    fn screenshot_regions_round_trip_without_a_schema_change() {
        let matcher = MkWindowMatcher {
            title: Some("Editor".into()),
            ..Default::default()
        };
        let regions = vec![
            SearchRegion::Rectangle {
                rect: crate::mkmacro::ScreenRect::new(-1600, -120, 3200, 900),
            },
            SearchRegion::Monitor { index: 3 },
            SearchRegion::Window {
                matcher: matcher.clone(),
            },
            SearchRegion::ClientArea { matcher },
        ];
        let steps = regions
            .iter()
            .cloned()
            .enumerate()
            .map(|(id, region)| MkStep {
                metadata: Default::default(),
                id: id as u64 + 1,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: MkErrorPolicy::Stop,
                action: MkAction::CaptureScreenshot(MkScreenshotPayload {
                    region,
                    destination: MkScreenshotDestination::Clipboard,
                    path: None,
                    format: MkScreenshotFormat::Png,
                    collision: MkFileCollisionPolicy::Error,
                    path_output: None,
                }),
            })
            .collect();
        let document = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            folders: vec![],
            macros: vec![MkMacro {
                signature: Default::default(),
                id: 7,
                name: "screenshots".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: MkPlayback::default(),
                steps,
            }],
            settings: MkMacroSettings::default(),
        };
        let json = serde_json::to_string(&document).unwrap();
        let loaded: MkMacroDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        let loaded_regions: Vec<_> = loaded.macros[0]
            .steps
            .iter()
            .map(|step| match &step.action {
                MkAction::CaptureScreenshot(payload) => payload.region.clone(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(loaded_regions, regions);
    }
}

#[cfg(test)]
mod prompt_payload_tests {
    use super::*;
    #[test]
    fn omitted_prompt_fields_use_compatible_defaults() {
        let action: MkAction =
            serde_json::from_str(r#"{"type":"prompt_input","data":{}}"#).unwrap();
        assert_eq!(
            action,
            MkAction::PromptInput(MkPromptInputPayload::default())
        );
    }
    #[test]
    fn prompt_action_round_trips_all_fields() {
        let action = MkAction::PromptInput(MkPromptInputPayload {
            title: "T".into(),
            prompt: "P".into(),
            default_value: "D".into(),
            variable: "_project2".into(),
            copy_to_clipboard: true,
        });
        assert_eq!(
            serde_json::from_str::<MkAction>(&serde_json::to_string(&action).unwrap()).unwrap(),
            action
        );
    }
    #[test]
    fn document_from_before_prompt_variant_still_loads() {
        let json = r#"{"schema_version":4,"macros":[{"id":1,"name":"old","steps":[{"id":1,"action":{"type":"delay","data":{"milliseconds":1}}}]}]}"#;
        let doc: MkMacroDocument = serde_json::from_str(json).unwrap();
        assert_eq!(doc.macros[0].steps.len(), 1);
    }
}

#[cfg(test)]
mod mouse_scroll_serialization_tests {
    use super::*;

    #[test]
    fn legacy_scroll_without_axis_defaults_to_vertical() {
        let action: MkAction =
            serde_json::from_str(r#"{"type":"mouse_scroll","data":{"i32_delta":-37}}"#).unwrap();
        assert_eq!(
            action,
            MkAction::MouseScroll {
                axis: MkMouseScrollAxis::Vertical,
                i32_delta: -37,
            }
        );
    }

    #[test]
    fn horizontal_scroll_round_trips_losslessly() {
        let action = MkAction::MouseScroll {
            axis: MkMouseScrollAxis::Horizontal,
            i32_delta: i32::MIN + 1,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""axis":"horizontal""#));
        assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
    }
}

#[cfg(test)]
mod notification_serialization_tests {
    use super::*;

    #[test]
    fn notification_enums_have_stable_values_and_round_trip() {
        for (kind, expected) in [
            (MkNotificationKind::Information, "information"),
            (MkNotificationKind::Success, "success"),
            (MkNotificationKind::Warning, "warning"),
            (MkNotificationKind::Error, "error"),
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
            assert_eq!(
                serde_json::from_str::<MkNotificationKind>(&json).unwrap(),
                kind
            );
        }
        for duration in [MkNotificationDuration::Short, MkNotificationDuration::Long] {
            let json = serde_json::to_string(&duration).unwrap();
            assert_eq!(
                serde_json::from_str::<MkNotificationDuration>(&json).unwrap(),
                duration
            );
        }
        let omitted: MkNotifyPayload = serde_json::from_str("{}").unwrap();
        assert_eq!(omitted, MkNotifyPayload::default());
    }

    #[test]
    fn new_actions_have_stable_tags_and_round_trip_losslessly() {
        let actions = [
            MkAction::Notify(MkNotifyPayload {
                title: "Finished ${job}".into(),
                description: "Everything worked".into(),
                kind: MkNotificationKind::Success,
                duration: MkNotificationDuration::Long,
                show_symbol: false,
            }),
            MkAction::PlaySound(MkPlaySoundPayload {
                sound: "Alarm03.wav".into(),
            }),
        ];
        for (action, tag) in actions.into_iter().zip(["notify", "play_sound"]) {
            let json = serde_json::to_string(&action).unwrap();
            assert!(json.contains(&format!(r#""type":"{tag}""#)));
            assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
        }
    }
}

#[cfg(test)]
mod breakpoint_serialization_tests {
    use super::*;

    fn step(breakpoint: bool) -> MkStep {
        MkStep {
            metadata: Default::default(),
            id: 2,
            enabled: true,
            breakpoint,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action: MkAction::Delay(MkDelayPayload {
                fixed_ms: 25,
                ..Default::default()
            }),
        }
    }

    #[test]
    fn omitted_breakpoint_defaults_to_false() {
        let mut json = serde_json::to_value(step(true)).unwrap();
        json.as_object_mut().unwrap().remove("breakpoint");

        let decoded: MkStep = serde_json::from_value(json).unwrap();

        assert!(!decoded.breakpoint);
    }

    #[test]
    fn breakpoint_values_round_trip_through_json() {
        for breakpoint in [true, false] {
            let expected = step(breakpoint);
            let json = serde_json::to_string(&expected).unwrap();

            assert_eq!(serde_json::from_str::<MkStep>(&json).unwrap(), expected);
        }
    }

    #[test]
    fn breakpoint_is_serialized_as_persisted_step_data() {
        let document = MkMacroDocument {
            macros: vec![MkMacro {
                signature: Default::default(),
                id: 1,
                name: "Debug authoring".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: MkHotkeyScope::default(),
                folder_id: None,
                playback: MkPlayback::default(),
                steps: vec![step(true)],
            }],
            ..Default::default()
        };
        let json = serde_json::to_value(document).unwrap();

        assert_eq!(
            json.pointer("/macros/0/steps/0/breakpoint"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(json.get("breakpoint").is_none());
        assert!(json.pointer("/macros/0/breakpoint").is_none());
        assert!(json.pointer("/macros/0/playback/breakpoint").is_none());
    }
}

#[cfg(test)]
mod wait_timeout_tests {
    use super::*;

    #[test]
    fn timeout_duration_distinguishes_indefinite_and_finite_waits() {
        assert_eq!(
            MkWaitOptions {
                timeout_ms: 0,
                poll_interval_ms: 25,
            }
            .timeout_duration(),
            None
        );
        assert_eq!(
            MkWaitOptions {
                timeout_ms: 1_234,
                poll_interval_ms: 25,
            }
            .timeout_duration(),
            Some(Duration::from_millis(1_234))
        );
        let visual = WaitForVisualChange {
            timeout_ms: 0,
            ..WaitForVisualChange::default()
        };
        assert_eq!(visual.timeout_duration(), None);
    }
}

#[cfg(test)]
mod schema_v10_serialization_tests {
    use super::*;

    #[test]
    fn document_defaults_to_current_schema_and_no_folders() {
        let document: MkMacroDocument = serde_json::from_str("{}").unwrap();
        assert_eq!(document.schema_version, SCHEMA_VERSION);
        assert!(document.folders.is_empty());
        assert!(MkMacroDocument::default().folders.is_empty());
    }

    fn sample_macro() -> MkMacro {
        MkMacro {
            signature: Default::default(),
            id: 7,
            name: "Scoped".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: MkHotkeyScope::default(),
            folder_id: Some(42),
            playback: MkPlayback::default(),
            steps: vec![],
        }
    }

    #[test]
    fn folders_and_macro_folder_ids_round_trip() {
        let document = MkMacroDocument {
            folders: vec![MkMacroFolder {
                id: 42,
                name: "Work".into(),
            }],
            macros: vec![sample_macro()],
            ..Default::default()
        };
        let json = serde_json::to_string(&document).unwrap();
        assert_eq!(
            serde_json::from_str::<MkMacroDocument>(&json).unwrap(),
            document
        );
    }

    #[test]
    fn hotkey_scopes_default_and_round_trip() {
        let json = serde_json::to_value(sample_macro()).unwrap();
        let mut legacy = json.as_object().unwrap().clone();
        legacy.remove("hotkey_scope");
        let decoded: MkMacro = serde_json::from_value(legacy.into()).unwrap();
        assert_eq!(decoded.hotkey_scope, MkHotkeyScope::AnyWindow);

        let scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Editor".into()),
            process: Some("editor.exe".into()),
            ..Default::default()
        });
        let json = serde_json::to_string(&scope).unwrap();
        assert!(json.contains(r#""type":"active_window""#));
        assert_eq!(serde_json::from_str::<MkHotkeyScope>(&json).unwrap(), scope);
    }

    #[test]
    fn fixed_and_random_delays_round_trip() {
        let delays = [
            MkDelayPayload::default(),
            MkDelayPayload {
                mode: MkDelayMode::RandomRange,
                fixed_ms: 1000,
                minimum_ms: 25,
                maximum_ms: 75,
            },
        ];
        for payload in delays {
            let action = MkAction::Delay(payload);
            let json = serde_json::to_string(&action).unwrap();
            assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
        }
    }

    #[test]
    fn click_within_signed_region_round_trips() {
        let action = MkAction::ClickWithinRegion(MkClickWithinRegionPayload {
            rect: ScreenRect::new(-1920, -240, 640, 480),
            button: MkMouseButton::Right,
            clicks: 2,
            edge_padding_px: 12,
        });
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""x":-1920"#));
        assert!(json.contains(r#""y":-240"#));
        assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
    }

    #[test]
    fn go_to_desktop_persists_one_based_number() {
        let action = MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 3 });
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(
            json,
            r#"{"type":"virtual_desktop","data":{"go_to":{"desktop":3}}}"#
        );
        assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
    }
}

#[cfg(test)]
mod ocr_payload_tests {
    use super::*;

    #[test]
    fn ocr_payload_defaults_match_authoring_contract() {
        let find = MkOcrFindPayload::default();
        assert_eq!(find.search, MkOcrSearchSpec::default());
        assert_eq!(find.wait.timeout_ms, 5_000);
        assert_eq!(find.wait.poll_interval_ms, 250);
        assert_eq!(find.not_found_policy, MkImageNotFoundPolicy::Continue);

        let click = MkOcrClickPayload::default();
        assert_eq!(click.wait, find.wait);
        assert_eq!(click.not_found_policy, MkImageNotFoundPolicy::Fail);
        assert_eq!(click.button, MkMouseButton::Left);
        assert_eq!(click.clicks, 1);
        assert_eq!((click.x_offset, click.y_offset), (0, 0));

        assert_eq!(MkOcrOccurrence::First.selected_index(), Some(0));
        assert_eq!(MkOcrOccurrence::Nth(1).selected_index(), Some(0));
        assert_eq!(MkOcrOccurrence::Nth(3).selected_index(), Some(2));
        assert_eq!(MkOcrOccurrence::Nth(0).selected_index(), None);
    }

    fn variable_condition() -> MkCondition {
        MkCondition::Variable {
            name: "ready".into(),
            op: MkCompareOp::Eq,
            value: MkValue::Boolean(true),
        }
    }

    fn ocr_condition() -> MkCondition {
        MkCondition::OcrTextSearch {
            search: MkOcrSearchCondition::default(),
            found: true,
        }
    }

    #[test]
    fn contains_ocr_recurses_through_logical_conditions() {
        assert!(ocr_condition().contains_ocr());
        assert!(
            MkCondition::All {
                conditions: vec![variable_condition(), ocr_condition()],
            }
            .contains_ocr()
        );
        assert!(
            MkCondition::Any {
                conditions: vec![variable_condition(), ocr_condition()],
            }
            .contains_ocr()
        );
        assert!(
            MkCondition::Not {
                condition: Box::new(ocr_condition()),
            }
            .contains_ocr()
        );

        assert!(
            !MkCondition::All {
                conditions: vec![
                    variable_condition(),
                    MkCondition::WindowExists {
                        matcher: MkWindowMatcher::default(),
                    },
                ],
            }
            .contains_ocr()
        );
        assert!(
            !MkCondition::Not {
                condition: Box::new(variable_condition()),
            }
            .contains_ocr()
        );
    }

    #[test]
    fn ocr_outputs_trim_names_and_remove_empty_values() {
        let mut outputs = MkOcrOutputs {
            found: Some(" found ".into()),
            matched_text: Some("\tmatched\n".into()),
            point: Some(" ".into()),
            x: None,
            y: Some(" y".into()),
            match_count: Some(String::new()),
        };
        outputs.normalize();
        assert_eq!(outputs.found.as_deref(), Some("found"));
        assert_eq!(outputs.matched_text.as_deref(), Some("matched"));
        assert_eq!(outputs.point, None);
        assert_eq!(outputs.x, None);
        assert_eq!(outputs.y.as_deref(), Some("y"));
        assert_eq!(outputs.match_count, None);
    }

    #[test]
    fn every_ocr_action_condition_mode_and_occurrence_round_trips() {
        for language in [
            MkOcrLanguage::Auto,
            MkOcrLanguage::LanguageTag("en-GB".into()),
        ] {
            let json = serde_json::to_string(&language).unwrap();
            assert_eq!(
                serde_json::from_str::<MkOcrLanguage>(&json).unwrap(),
                language
            );
        }
        for mode in [
            MkOcrMatchMode::Contains,
            MkOcrMatchMode::WholeWordPhrase,
            MkOcrMatchMode::Regex,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(serde_json::from_str::<MkOcrMatchMode>(&json).unwrap(), mode);
        }
        for occurrence in [MkOcrOccurrence::First, MkOcrOccurrence::Nth(4)] {
            let json = serde_json::to_string(&occurrence).unwrap();
            assert_eq!(
                serde_json::from_str::<MkOcrOccurrence>(&json).unwrap(),
                occurrence
            );
        }

        let search = MkOcrSearchSpec {
            text: "${needle}".into(),
            region: SearchRegion::Rectangle {
                rect: ScreenRect::new(-1200, 40, 900, 500),
            },
            language: MkOcrLanguage::LanguageTag("ja-JP".into()),
            match_mode: MkOcrMatchMode::WholeWordPhrase,
            case_sensitive: true,
            occurrence: MkOcrOccurrence::Nth(2),
        };
        let actions = [
            MkAction::OcrFindText(MkOcrFindPayload {
                search: search.clone(),
                outputs: MkOcrOutputs {
                    found: Some("found".into()),
                    matched_text: Some("matched".into()),
                    point: Some("point".into()),
                    x: Some("x".into()),
                    y: Some("y".into()),
                    match_count: Some("count".into()),
                },
                ..Default::default()
            }),
            MkAction::OcrClickText(MkOcrClickPayload {
                search: MkOcrSearchSpec {
                    match_mode: MkOcrMatchMode::Regex,
                    ..search.clone()
                },
                button: MkMouseButton::Right,
                clicks: 2,
                x_offset: -4,
                y_offset: 7,
                ..Default::default()
            }),
            MkAction::OcrReadText(MkOcrReadPayload {
                region: SearchRegion::Desktop,
                language: MkOcrLanguage::Auto,
                output_variable: "page_text".into(),
            }),
        ];
        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
        }

        let condition = MkCondition::OcrTextSearch {
            search: MkOcrSearchCondition { search },
            found: false,
        };
        let json = serde_json::to_string(&condition).unwrap();
        assert!(json.contains(r#""type":"ocr_text_search""#));
        assert_eq!(
            serde_json::from_str::<MkCondition>(&json).unwrap(),
            condition
        );
    }
}
