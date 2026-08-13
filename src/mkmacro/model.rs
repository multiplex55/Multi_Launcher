use crate::mkmacro::variables::{MkPoint, MkValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;
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
}
impl Default for MkMacroDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            macros: vec![],
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
pub enum MkErrorPolicy {
    Stop,
    Continue,
    Retry(MkRetry),
}
impl Default for MkErrorPolicy {
    fn default() -> Self {
        Self::Stop
    }
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
    Screen { point: MkPoint },
    ActiveWindow { point: MkPoint },
    Variable { name: String },
    Image { asset_id: u64, offset: MkPoint },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkUiSelector {
    #[serde(default)]
    pub automation_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub control_type: Option<String>,
    #[serde(default)]
    pub ancestor: Option<Box<MkUiSelector>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkWaitOptions {
    pub timeout_ms: u64,
    pub poll_interval_ms: u64,
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
    ImageResult {
        asset_id: u64,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkTextPayload {
    pub text: String,
    pub mode: MkTextMode,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkMousePayload {
    pub target: MkCoordinateTarget,
    pub button: MkMouseButton,
    #[serde(default = "one")]
    pub clicks: u32,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkImagePayload {
    pub asset_id: u64,
    pub wait: MkWaitOptions,
    #[serde(default)]
    pub confidence: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkUiPayload {
    pub window: MkWindowMatcher,
    pub selector: MkUiSelector,
    #[serde(default)]
    pub wait: Option<MkWaitOptions>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MkAction {
    KeyDown(MkKey),
    KeyUp(MkKey),
    KeyPress(MkKey),
    Hotkey(Vec<MkKey>),
    Text(MkTextPayload),
    MouseMove(MkCoordinateTarget),
    MouseClick(MkMousePayload),
    MouseDown(MkMouseButton),
    MouseUp(MkMouseButton),
    MouseScroll {
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
    UiWait(MkUiPayload),
}
impl MkAction {
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
