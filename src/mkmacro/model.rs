use super::{
    image_search::{AlphaPolicy, ReturnPoint},
    screen::SearchRegion,
};
use crate::mkmacro::variables::{MkPoint, MkValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

// Schema 7 introduces action tags that schema-6 builds do not know how to
// deserialize, so documents containing them must not claim schema-6 compatibility.
pub const SCHEMA_VERSION: u32 = 7;
fn schema() -> u32 {
    SCHEMA_VERSION
}
fn yes() -> bool {
    true
}
fn one() -> u32 {
    1
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMacroDocument {
    #[serde(default = "schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub macros: Vec<MkMacro>,
    /// Document-wide authoring controls shared by every macro.
    #[serde(default)]
    pub settings: MkMacroSettings,
}
impl Default for MkMacroDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            macros: vec![],
            settings: MkMacroSettings::default(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMacroSettings {
    pub record_toggle_hotkey: MkHotkey,
}
impl Default for MkMacroSettings {
    fn default() -> Self {
        Self {
            record_toggle_hotkey: MkHotkey {
                key: MkKey::Function(9),
                modifiers: vec![],
            },
        }
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
    pub playback: MkPlayback,
    #[serde(default)]
    pub steps: Vec<MkStep>,
    /// Reference images owned by this macro. IDs remain the persisted action reference.
    #[serde(default)]
    pub image_assets: Vec<MkImageAsset>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkStep {
    #[serde(default)]
    pub id: u64,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "one")]
    pub repeat: u32,
    #[serde(default)]
    pub delay_after_ms: u64,
    #[serde(default)]
    pub on_error: MkErrorPolicy,
    pub action: MkAction,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkHotkey {
    pub key: MkKey,
    #[serde(default)]
    pub modifiers: Vec<MkKey>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkKey {
    Character(String),
    Enter,
    Tab,
    Escape,
    Space,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
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
        asset_id: u64,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkImageAsset {
    #[serde(default)]
    pub id: u64,
    pub name: String,
    pub relative_path: String,
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
    PreviousImageResult {
        asset_id: Option<u64>,
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
/// A single, immediate image search used by a condition.  Action polling and
/// output policy deliberately live in [`MkImagePayload`], not here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkImageSearchCondition {
    pub asset_id: u64,
    #[serde(default)]
    pub region: SearchRegion,
    #[serde(default)]
    pub tolerance: u8,
    #[serde(default)]
    pub alpha: AlphaPolicy,
    #[serde(default)]
    pub return_point: ReturnPoint,
}

impl MkImageSearchCondition {
    pub fn as_payload(&self) -> MkImagePayload {
        MkImagePayload {
            asset_id: self.asset_id,
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
    pub asset_id: u64,
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
    MouseDown(MkMouseButton),
    MouseUp(MkMouseButton),
    MouseScroll {
        #[serde(default)]
        axis: MkMouseScrollAxis,
        i32_delta: i32,
    },
    Delay {
        milliseconds: u64,
    },
    Process(MkProcessPayload),
    LauncherCommand {
        command: String,
        args: Option<String>,
    },
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
mod image_payload_tests {
    use super::*;
    #[test]
    fn missing_image_search_fields_default_and_current_round_trips() {
        let legacy = r#"{"asset_id":4,"wait":{"timeout_ms":10,"poll_interval_ms":2}}"#;
        let p: MkImagePayload = serde_json::from_str(legacy).unwrap();
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
        let mut p: MkImagePayload =
            serde_json::from_str(r#"{"asset_id":4,"wait":{"timeout_ms":10,"poll_interval_ms":2}}"#)
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
                id: id as u64 + 1,
                enabled: true,
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
            macros: vec![MkMacro {
                id: 7,
                name: "screenshots".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: MkPlayback::default(),
                steps,
                image_assets: vec![],
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
