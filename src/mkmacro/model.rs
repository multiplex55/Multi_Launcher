use super::{
    image_search::{AlphaPolicy, ReturnPoint},
    screen::SearchRegion,
};
use crate::mkmacro::variables::{MkPoint, MkValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 4;
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MkAction {
    KeyDown(MkKey),
    KeyUp(MkKey),
    KeyPress(MkKey),
    Hotkey(Vec<MkKey>),
    Text(MkTextPayload),
    MouseMove(MkMouseMovePayload),
    MouseDrag(MkMouseDragPayload),
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
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<MkImagePayload>(&json).unwrap(), p);
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
