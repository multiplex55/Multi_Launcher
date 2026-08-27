use super::MkMacroDialog;
use crate::mkmacro::variables::{MkPoint, MkValue};
use crate::mkmacro::*;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionCategory {
    KeyboardText,
    Mouse,
    Timing,
    Notifications,
    Windows,
    ProgramsLauncher,
    Logic,
    Variables,
    Visual,
    UiAutomation,
}
impl ActionCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::KeyboardText => "Keyboard & Text",
            Self::Mouse => "Mouse",
            Self::Timing => "Timing",
            Self::Notifications => "Notifications",
            Self::Windows => "Windows",
            Self::ProgramsLauncher => "Programs & Launcher",
            Self::Logic => "Logic",
            Self::Variables => "Variables",
            Self::Visual => "Visual",
            Self::UiAutomation => "UI Automation",
        }
    }

    /// Product-level palette switch. Keeping this explicit makes catalog tests
    /// fail at the descriptor boundary if a disabled category leaks into UI.
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::UiAutomation)
    }
}

/// Product-defined order for action-palette headings. This is deliberately
/// independent of the order in which descriptors are declared.
pub const PALETTE_CATEGORY_ORDER: &[ActionCategory] = &[
    ActionCategory::KeyboardText,
    ActionCategory::Mouse,
    ActionCategory::Timing,
    // Notifications are workflow actions, not screen-inspection actions. Keep
    // their product position explicitly between Timing and Programs/Windows.
    ActionCategory::Notifications,
    ActionCategory::Visual,
    ActionCategory::Windows,
    ActionCategory::ProgramsLauncher,
    ActionCategory::Variables,
    ActionCategory::Logic,
    // Retain disabled categories in the model so enabling one is an explicit,
    // ordering-aware product decision. Availability still controls rendering.
    ActionCategory::UiAutomation,
];

pub struct ActionDescriptor {
    pub category: ActionCategory,
    pub availability: ActionAvailability,
    pub name: &'static str,
    pub description: &'static str,
    pub keywords: &'static [&'static str],
    pub make_default: fn() -> MkAction,
    pub editor: EditorKind,
    pub runtime: RuntimeAvailability,
    /// A deliberate product/capability reason is mandatory for hidden rows.
    pub hidden_reason: Option<&'static str>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorKind {
    Keyboard,
    Text,
    MouseMove,
    MouseClick,
    MouseDrag,
    MouseButton,
    MouseScroll,
    Timing,
    Window,
    Process,
    Launcher,
    Image,
    Screenshot,
    Pixel,
    Condition,
    Repeat,
    Variable,
    PromptInput,
    Notify,
    PlaySound,
    General,
    DirectInsert,
}

impl EditorKind {
    /// Whether this strategy has a complete authoring transaction.  Keeping
    /// this beside the enum prevents callers from inventing editor exceptions.
    pub fn contract(self) -> Option<EditorContract> {
        editor_contract(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionAvailability {
    Ready,
    Hidden,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeAvailability {
    Supported,
    Unavailable,
}
#[derive(Clone, Copy)]
pub enum StructuralInsertion {
    If,
    Repeat,
    While,
}
fn point() -> MkCoordinateTarget {
    MkCoordinateTarget::Screen {
        point: MkPoint { x: 0, y: 0 },
    }
}
fn cond() -> MkCondition {
    MkCondition::All { conditions: vec![] }
}
fn image_cond(found: bool) -> MkCondition {
    MkCondition::ImageSearch {
        search: MkImageSearchCondition {
            // Required-asset draft sentinel; Apply remains disabled until the
            // user imports, captures, or selects a reference image.
            asset_id: 0,
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
        },
        found,
    }
}
fn wait() -> MkWaitOptions {
    MkWaitOptions {
        timeout_ms: 5000,
        poll_interval_ms: 100,
    }
}
fn matcher() -> MkWindowMatcher {
    MkWindowMatcher {
        title: Some("Window".into()),
        title_regex: None,
        process: None,
        class: None,
    }
}
fn wp() -> MkWindowPayload {
    MkWindowPayload {
        matcher: matcher(),
        wait: Some(wait()),
    }
}
fn ip(not_found_policy: MkImageNotFoundPolicy) -> MkImagePayload {
    MkImagePayload {
        // Zero is the documented editor-draft sentinel. The editor requires an
        // imported/captured asset before enabling Apply.
        asset_id: 0,
        wait: wait(),
        region: SearchRegion::Desktop,
        tolerance: 0,
        alpha: AlphaPolicy::Compare,
        return_point: ReturnPoint::Center,
        not_found_policy,
        outputs: MkImageOutputs::default(),
    }
}
fn up() -> MkUiPayload {
    MkUiPayload {
        window: matcher(),
        selector: MkUiSelector {
            automation_id: None,
            name: Some(String::new()),
            control_type: None,
            class_name: None,
            framework_id: None,
            ancestor_path: vec![],
        },
        wait: Some(wait()),
    }
}
macro_rules! d {
    ($c:ident,$n:literal,$desc:literal,$keys:expr,$editor:ident,$a:expr) => {
        ActionDescriptor {
            category: ActionCategory::$c,
            availability: ActionAvailability::Ready,
            name: $n,
            description: $desc,
            keywords: $keys,
            make_default: || $a,
            editor: EditorKind::$editor,
            runtime: runtime_availability(&$a),
            hidden_reason: None,
        }
    };
    (hidden_ready,$c:ident,$n:literal,$desc:literal,$keys:expr,$editor:ident,$reason:literal,$a:expr) => {
        ActionDescriptor {
            category: ActionCategory::$c,
            availability: ActionAvailability::Hidden,
            name: $n,
            description: $desc,
            keywords: $keys,
            make_default: || $a,
            editor: EditorKind::$editor,
            runtime: runtime_availability(&$a),
            hidden_reason: Some($reason),
        }
    };
    (hidden,$c:ident,$n:literal,$desc:literal,$keys:expr,$editor:ident,$reason:literal,$a:expr) => {
        ActionDescriptor {
            category: ActionCategory::$c,
            availability: ActionAvailability::Hidden,
            name: $n,
            description: $desc,
            keywords: $keys,
            make_default: || $a,
            editor: EditorKind::$editor,
            runtime: runtime_availability(&$a),
            hidden_reason: Some($reason),
        }
    };
    (direct,$c:ident,$n:literal,$desc:literal,$keys:expr,$a:expr) => {
        ActionDescriptor {
            category: ActionCategory::$c,
            // These advanced markers are offered as context-checked structural
            // insertions and deliberately never open a configuration editor.
            availability: ActionAvailability::Ready,
            name: $n,
            description: $desc,
            keywords: $keys,
            make_default: || $a,
            editor: EditorKind::DirectInsert,
            runtime: runtime_availability(&$a),
            hidden_reason: None,
        }
    };
}

fn runtime_availability(action: &MkAction) -> RuntimeAvailability {
    if crate::mkmacro::executor::has_runtime_support(action) {
        RuntimeAvailability::Supported
    } else {
        RuntimeAvailability::Unavailable
    }
}
pub fn descriptors() -> Vec<ActionDescriptor> {
    let entries = vec![
        d!(
            KeyboardText,
            "Key Press",
            "Press and release a keyboard key",
            &["keyboard", "send"],
            Keyboard,
            MkAction::KeyPress(MkKey::Enter)
        ),
        d!(
            KeyboardText,
            "Key Down",
            "Hold a keyboard key",
            &["keyboard"],
            Keyboard,
            MkAction::KeyDown(MkKey::Enter)
        ),
        d!(
            KeyboardText,
            "Key Up",
            "Release a keyboard key",
            &["keyboard"],
            Keyboard,
            MkAction::KeyUp(MkKey::Enter)
        ),
        d!(
            KeyboardText,
            "Hotkey",
            "Send a key combination",
            &["keyboard", "send"],
            Keyboard,
            MkAction::Hotkey(vec![MkKey::Control, MkKey::Character("C".into())])
        ),
        d!(
            KeyboardText,
            "Text",
            "Type or paste text",
            &["type", "send", "keyboard"],
            Text,
            MkAction::Text(MkTextPayload {
                text: String::new(),
                mode: MkTextMode::Type
            })
        ),
        d!(
            Mouse,
            "Mouse Move",
            "Move the pointer",
            &["mouse"],
            MouseMove,
            MkAction::MouseMove(MkMouseMovePayload {
                target: point(),
                duration_ms: 0
            })
        ),
        d!(
            Mouse,
            "Mouse Drag",
            "Drag while holding a mouse button",
            &["mouse", "drag"],
            MouseDrag,
            MkAction::MouseDrag(MkMouseDragPayload {
                from: point(),
                to: point(),
                button: MkMouseButton::Left,
                duration_ms: 400
            })
        ),
        d!(
            Mouse,
            "Mouse Click",
            "Click a mouse button",
            &["click", "mouse"],
            MouseClick,
            MkAction::MouseClick(MkMousePayload {
                target: point(),
                button: MkMouseButton::Left,
                clicks: 1
            })
        ),
        d!(
            Mouse,
            "Mouse Down",
            "Hold a mouse button",
            &["mouse"],
            MouseButton,
            MkAction::MouseDown(MkMouseButton::Left)
        ),
        d!(
            Mouse,
            "Mouse Up",
            "Release a mouse button",
            &["mouse"],
            MouseButton,
            MkAction::MouseUp(MkMouseButton::Left)
        ),
        d!(
            Mouse,
            "Mouse Scroll",
            "Scroll the mouse wheel",
            &["mouse"],
            MouseScroll,
            MkAction::MouseScroll {
                axis: MkMouseScrollAxis::Vertical,
                i32_delta: -120,
            }
        ),
        d!(
            Timing,
            "Delay",
            "Wait for a duration",
            &["wait"],
            Timing,
            MkAction::Delay { milliseconds: 1000 }
        ),
        d!(
            Notifications,
            "Notify",
            "Display a silent Windows notification",
            &["notification", "toast", "message", "alert"],
            Notify,
            MkAction::Notify(MkNotifyPayload::default())
        ),
        d!(
            Notifications,
            "Play Sound",
            "Start sound playback and continue the macro",
            &["audio", "sound", "alarm", "reminder"],
            PlaySound,
            MkAction::PlaySound(MkPlaySoundPayload::default())
        ),
        d!(
            Timing,
            "Wait Until",
            "Wait for a condition",
            &["wait", "condition"],
            Condition,
            MkAction::WaitUntil {
                condition: cond(),
                wait: wait()
            }
        ),
        d!(
            Visual,
            "Wait for Image",
            "Wait until a reference image becomes visible",
            &[
                "wait",
                "image",
                "visual",
                "appear",
                "visible",
                "disappear",
                "gone"
            ],
            Condition,
            MkAction::WaitUntil {
                condition: image_cond(true),
                wait: wait()
            }
        ),
        d!(
            Visual,
            "Wait for Image to Disappear",
            "Wait until a reference image is no longer visible",
            &[
                "wait",
                "image",
                "visual",
                "appear",
                "visible",
                "disappear",
                "gone"
            ],
            Condition,
            MkAction::WaitUntil {
                condition: image_cond(false),
                wait: wait()
            }
        ),
        d!(
            Windows,
            "Activate Window",
            "Activate a matching window",
            &["window"],
            Window,
            MkAction::WindowActivate(wp())
        ),
        d!(
            Windows,
            "Close Window",
            "Close a matching window",
            &["window"],
            Window,
            MkAction::WindowClose(matcher())
        ),
        d!(
            Windows,
            "Wait for Window",
            "Wait for a matching window",
            &["window", "wait"],
            Window,
            MkAction::WindowWait(wp())
        ),
        d!(
            Windows,
            "Move / Resize Window",
            "Move or resize a matching window",
            &["window", "move", "position", "resize", "size"],
            Window,
            MkAction::WindowMoveResize(MkWindowMoveResizePayload {
                matcher: matcher(),
                x: Some(0),
                y: Some(0),
                width: None,
                height: None
            })
        ),
        d!(
            Windows,
            "Minimize Window",
            "Minimize a matching window",
            &["window", "minimize"],
            Window,
            MkAction::WindowState {
                matcher: matcher(),
                state: MkWindowState::Minimize
            }
        ),
        d!(
            direct,
            Windows,
            "Create Virtual Desktop",
            "Create a new Windows virtual desktop",
            &["desktop", "workspace", "create", "new"],
            MkAction::VirtualDesktop(MkVirtualDesktopAction::Create)
        ),
        d!(
            direct,
            Windows,
            "Switch Virtual Desktop Left",
            "Switch to the virtual desktop on the left",
            &["desktop", "workspace", "left", "previous"],
            MkAction::VirtualDesktop(MkVirtualDesktopAction::SwitchLeft)
        ),
        d!(
            direct,
            Windows,
            "Switch Virtual Desktop Right",
            "Switch to the virtual desktop on the right",
            &["desktop", "workspace", "right", "next"],
            MkAction::VirtualDesktop(MkVirtualDesktopAction::SwitchRight)
        ),
        d!(
            direct,
            Windows,
            "Close Current Virtual Desktop",
            "Close the current virtual desktop using native Windows behavior",
            &["desktop", "workspace", "close"],
            MkAction::VirtualDesktop(MkVirtualDesktopAction::CloseCurrent)
        ),
        d!(
            Windows,
            "Maximize Window",
            "Maximize a matching window",
            &["window", "maximize"],
            Window,
            MkAction::WindowState {
                matcher: matcher(),
                state: MkWindowState::Maximize
            }
        ),
        d!(
            Windows,
            "Restore Window",
            "Restore a matching window",
            &["window", "restore"],
            Window,
            MkAction::WindowState {
                matcher: matcher(),
                state: MkWindowState::Restore
            }
        ),
        d!(
            ProgramsLauncher,
            "Run Program",
            "Start a program",
            &["run", "launch"],
            Process,
            MkAction::Process(MkProcessPayload {
                program: "program".into(),
                arguments: vec![],
                working_directory: None,
                wait: false
            })
        ),
        d!(
            ProgramsLauncher,
            "Launcher Command",
            "Run a launcher command",
            &["run", "launch"],
            Launcher,
            MkAction::LauncherCommand {
                command: "command".into(),
                args: None
            }
        ),
        d!(
            Variables,
            "Set Variable",
            "Store a value",
            &["variable"],
            Variable,
            MkAction::SetVariable {
                name: "value".into(),
                value: MkValue::Null
            }
        ),
        d!(
            Variables,
            "Unset Variable",
            "Remove a value",
            &["variable"],
            Variable,
            MkAction::UnsetVariable {
                name: "value".into()
            }
        ),
        d!(
            Variables,
            "Prompt for Input",
            "Ask for text and store it in a variable",
            &["prompt", "input", "ask", "variable", "clipboard"],
            PromptInput,
            MkAction::PromptInput(MkPromptInputPayload::default())
        ),
        d!(
            Logic,
            "If",
            "Condition block",
            &["condition"],
            Condition,
            MkAction::If(cond())
        ),
        d!(
            direct,
            Logic,
            "Else",
            "Alternate condition branch",
            &["condition"],
            MkAction::Else
        ),
        d!(
            direct,
            Logic,
            "End If",
            "End condition block",
            &["condition"],
            MkAction::EndIf
        ),
        d!(
            Logic,
            "Repeat",
            "Repeat block",
            &["loop"],
            Repeat,
            MkAction::RepeatStart { count: 5 }
        ),
        d!(
            direct,
            Logic,
            "End Repeat",
            "End repeat block",
            &["loop"],
            MkAction::RepeatEnd
        ),
        d!(
            Logic,
            "While",
            "Conditional loop",
            &["loop", "condition"],
            Condition,
            MkAction::WhileStart { condition: cond() }
        ),
        d!(
            direct,
            Logic,
            "End While",
            "End while loop",
            &["loop"],
            MkAction::WhileEnd
        ),
        d!(
            direct,
            Logic,
            "Break",
            "Exit a loop",
            &["loop"],
            MkAction::Break
        ),
        d!(
            direct,
            Logic,
            "Continue",
            "Continue a loop",
            &["loop"],
            MkAction::Continue
        ),
        d!(
            Visual,
            "Find Image",
            "Find an image",
            &["image"],
            Image,
            MkAction::ImageFind(ip(MkImageNotFoundPolicy::Continue))
        ),
        d!(
            Visual,
            "Capture Screenshot",
            "Capture a desktop, monitor, rectangle, window, or client area",
            &["screenshot", "capture", "clipboard", "image"],
            Screenshot,
            MkAction::CaptureScreenshot(MkScreenshotPayload {
                region: SearchRegion::Desktop,
                destination: MkScreenshotDestination::File,
                path: Some("screenshots/screenshot.png".into()),
                format: MkScreenshotFormat::Png,
                collision: MkFileCollisionPolicy::Unique,
                path_output: None,
            })
        ),
        d!(
            Visual,
            "Wait for Visual Change",
            "Wait until a percentage of pixels changes from the initial frame",
            &["wait", "change", "screen", "visual"],
            Screenshot,
            MkAction::WaitForVisualChange(WaitForVisualChange::default())
        ),
        d!(
            Visual,
            "Click Image",
            "Find and click an image",
            &["image", "click"],
            Image,
            MkAction::ImageClick(ip(MkImageNotFoundPolicy::Fail))
        ),
        d!(
            Visual,
            "Find Pixel Color",
            "Find the first matching pixel in a region",
            &["pixel", "color", "search"],
            Pixel,
            MkAction::FindPixel(MkPixelSearchPayload {
                search_id: 1,
                color: "#000000".into(),
                tolerance: 0,
                region: SearchRegion::Desktop,
                wait: MkWaitOptions::default(),
                not_found_policy: MkImageNotFoundPolicy::Continue,
                outputs: MkImageOutputs::default(),
            })
        ),
        d!(
            Visual,
            "Check Pixel Color",
            "Check the color at one pixel coordinate",
            &["pixel", "color", "screen"],
            Pixel,
            MkAction::PixelCheck {
                target: point(),
                color: "#000000".into(),
                tolerance: 0
            }
        ),
        d!(
            hidden,
            UiAutomation,
            "Invoke UI Element",
            "Invoke a UI element",
            &["uia"],
            General,
            "UI Automation execution and editing are not yet available",
            MkAction::UiInvoke(up())
        ),
        d!(
            hidden,
            UiAutomation,
            "Set UI Value",
            "Set an element value",
            &["uia"],
            General,
            "UI Automation execution and editing are not yet available",
            MkAction::UiSetValue {
                target: up(),
                value: String::new()
            }
        ),
        d!(
            hidden,
            UiAutomation,
            "Read UI Value",
            "Read an element value",
            &["uia"],
            General,
            "UI Automation execution and editing are not yet available",
            MkAction::UiReadValue {
                target: up(),
                variable: "value".into()
            }
        ),
        d!(
            hidden,
            UiAutomation,
            "Toggle UI Element",
            "Toggle an element",
            &["uia"],
            General,
            "UI Automation execution and editing are not yet available",
            MkAction::UiToggle(up())
        ),
        d!(
            hidden,
            UiAutomation,
            "Select UI Element",
            "Select an element",
            &["uia"],
            General,
            "UI Automation execution and editing are not yet available",
            MkAction::UiSelect(up())
        ),
        d!(
            hidden,
            UiAutomation,
            "Focus UI Element",
            "Focus an element",
            &["uia"],
            General,
            "UI Automation execution and editing are not yet available",
            MkAction::UiFocus(up())
        ),
        d!(
            hidden,
            UiAutomation,
            "Wait for UI Element",
            "Wait for an element",
            &["uia", "wait"],
            General,
            "UI Automation execution and editing are not yet available",
            MkAction::UiWait(up())
        ),
    ];
    entries
}
/// The editor capability for every model variant. This exhaustive match is the
/// compile-time maintenance point for action/editor coverage.
pub fn editor_for_action(action: &MkAction) -> EditorKind {
    match action {
        MkAction::KeyDown(_) | MkAction::KeyUp(_) | MkAction::KeyPress(_) | MkAction::Hotkey(_) => {
            EditorKind::Keyboard
        }
        MkAction::Text(_) => EditorKind::Text,
        MkAction::Notify(_) => EditorKind::Notify,
        MkAction::PlaySound(_) => EditorKind::PlaySound,
        MkAction::MouseMove(_) => EditorKind::MouseMove,
        MkAction::MouseDrag(_) => EditorKind::MouseDrag,
        MkAction::MouseClick(_) => EditorKind::MouseClick,
        MkAction::MouseDown(_) | MkAction::MouseUp(_) => EditorKind::MouseButton,
        MkAction::MouseScroll { .. } => EditorKind::MouseScroll,
        MkAction::Delay { .. } => EditorKind::Timing,
        MkAction::Process(_) => EditorKind::Process,
        MkAction::LauncherCommand { .. } => EditorKind::Launcher,
        MkAction::WindowActivate(_)
        | MkAction::WindowClose(_)
        | MkAction::WindowWait(_)
        | MkAction::WindowMoveResize(_)
        | MkAction::WindowState { .. } => EditorKind::Window,
        MkAction::VirtualDesktop(_) => EditorKind::DirectInsert,
        MkAction::If(_) | MkAction::WhileStart { .. } | MkAction::WaitUntil { .. } => {
            EditorKind::Condition
        }
        MkAction::RepeatStart { .. } => EditorKind::Repeat,
        MkAction::SetVariable { .. } | MkAction::UnsetVariable { .. } => EditorKind::Variable,
        MkAction::PromptInput(_) => EditorKind::PromptInput,
        MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatEnd
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue => EditorKind::DirectInsert,
        MkAction::ImageFind(_) | MkAction::ImageClick(_) => EditorKind::Image,
        MkAction::CaptureScreenshot(_) | MkAction::WaitForVisualChange(_) => EditorKind::Screenshot,
        MkAction::PixelCheck { .. } | MkAction::FindPixel(_) => EditorKind::Pixel,
        MkAction::UiInvoke(_)
        | MkAction::UiSetValue { .. }
        | MkAction::UiReadValue { .. }
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => EditorKind::General,
    }
}

/// True only when this exact action/editor pairing has an implemented route.
pub fn editor_route_recognizes(action: &MkAction, editor: EditorKind) -> bool {
    editor_for_action(action) == editor && editor_contract(editor).is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorContract {
    Configurable { field_count: usize },
    DirectInsert { context: InsertionContextRoute },
}

/// Static completeness declaration for a configurable editor.  This is kept
/// separate from transient widget state: an Apply button may be disabled while
/// required input is missing, but the feature itself must never be presented
/// as a permanently disabled/placeholder control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorCompleteness {
    pub has_primary_control: bool,
    pub intentionally_disabled: bool,
    pub placeholder_copy: Option<&'static str>,
}

pub fn editor_completeness(editor: EditorKind) -> Option<EditorCompleteness> {
    match editor {
        EditorKind::General => None,
        EditorKind::DirectInsert => Some(EditorCompleteness {
            has_primary_control: false,
            intentionally_disabled: false,
            placeholder_copy: None,
        }),
        EditorKind::Keyboard
        | EditorKind::Text
        | EditorKind::MouseMove
        | EditorKind::MouseClick
        | EditorKind::MouseDrag
        | EditorKind::MouseButton
        | EditorKind::MouseScroll
        | EditorKind::Timing
        | EditorKind::Window
        | EditorKind::Process
        | EditorKind::Launcher
        | EditorKind::Image
        | EditorKind::Screenshot
        | EditorKind::Pixel
        | EditorKind::Condition
        | EditorKind::Repeat
        | EditorKind::Variable
        | EditorKind::PromptInput
        | EditorKind::Notify
        | EditorKind::PlaySound => Some(EditorCompleteness {
            has_primary_control: true,
            intentionally_disabled: false,
            placeholder_copy: None,
        }),
    }
}

/// Contract for validating a freshly-created editor value. Image actions are
/// valid drafts before an asset is chosen, but cannot be committed until the
/// editor has attached an asset. All other defaults are commit-ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftValidationContract {
    CommitReady,
    AwaitingRequiredAsset,
}

pub fn draft_validation_contract(action: &MkAction) -> DraftValidationContract {
    match action {
        MkAction::ImageFind(payload) | MkAction::ImageClick(payload) if payload.asset_id == 0 => {
            DraftValidationContract::AwaitingRequiredAsset
        }
        MkAction::WaitUntil {
            condition: MkCondition::ImageSearch { search, .. },
            ..
        } if search.asset_id == 0 => DraftValidationContract::AwaitingRequiredAsset,
        _ => DraftValidationContract::CommitReady,
    }
}

/// Catalog presets may give a canonical action a more discoverable authoring
/// name without introducing a new persisted action variant.
pub fn descriptor_name_matches_action(descriptor: &ActionDescriptor, action: &MkAction) -> bool {
    descriptor.name == action_name(action)
        || matches!(
            (descriptor.name, action),
            (
                "Wait for Image" | "Wait for Image to Disappear",
                MkAction::WaitUntil {
                    condition: MkCondition::ImageSearch { .. },
                    ..
                }
            )
        )
}

/// Structural markers are complete actions at insertion time and intentionally
/// have no configurable fields. All other actions must pass through an editor.
pub fn requires_no_configuration(action: &MkAction) -> bool {
    matches!(
        action,
        MkAction::Else
            | MkAction::EndIf
            | MkAction::RepeatEnd
            | MkAction::WhileEnd
            | MkAction::Break
            | MkAction::Continue
            | MkAction::VirtualDesktop(_)
    )
}

/// The concrete validation path used before a structural marker is inserted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertionContextRoute {
    CompileCheckedStructuralPosition,
}

/// Shared product-copy guard used by catalog invariants.  This deliberately
/// complements (rather than replaces) the exact historical-string regression.
pub fn contains_placeholder_wording(value: &str) -> bool {
    const PLACEHOLDERS: [&str; 8] = [
        "existing specialized editor",
        "legacy action",
        "not implemented",
        "unavailable",
        "placeholder",
        "coming later",
        "coming soon",
        "todo",
    ];
    let value = value.to_ascii_lowercase();
    PLACEHOLDERS.iter().any(|wording| value.contains(wording))
}

/// Authoritative routing contract. Every configurable value listed here has a
/// concrete `action_ui` branch; `None` means that no editor route is implemented.
pub fn editor_contract(editor: EditorKind) -> Option<EditorContract> {
    match editor {
        EditorKind::DirectInsert => Some(EditorContract::DirectInsert {
            context: InsertionContextRoute::CompileCheckedStructuralPosition,
        }),
        EditorKind::General => None,
        EditorKind::Keyboard
        | EditorKind::Text
        | EditorKind::Timing
        | EditorKind::MouseButton
        | EditorKind::MouseScroll
        | EditorKind::Process
        | EditorKind::Launcher
        | EditorKind::Condition
        | EditorKind::Repeat
        | EditorKind::Variable => Some(EditorContract::Configurable { field_count: 1 }),
        EditorKind::PromptInput => Some(EditorContract::Configurable { field_count: 5 }),
        EditorKind::Notify => Some(EditorContract::Configurable { field_count: 5 }),
        EditorKind::PlaySound => Some(EditorContract::Configurable { field_count: 1 }),
        EditorKind::MouseMove | EditorKind::MouseClick | EditorKind::Image | EditorKind::Pixel => {
            Some(EditorContract::Configurable { field_count: 2 })
        }
        EditorKind::Screenshot => Some(EditorContract::Configurable { field_count: 6 }),
        EditorKind::MouseDrag | EditorKind::Window => {
            Some(EditorContract::Configurable { field_count: 3 })
        }
    }
}
/// Descriptors currently offered by the macro-authoring UI.
pub fn is_available_in_palette(descriptor: &ActionDescriptor) -> bool {
    descriptor.availability == ActionAvailability::Ready
        && descriptor.category.is_enabled()
        && descriptor.hidden_reason.is_none()
        && descriptor.runtime == RuntimeAvailability::Supported
}

pub fn visible_descriptors() -> impl Iterator<Item = ActionDescriptor> {
    descriptors().into_iter().filter(is_available_in_palette)
}

/// Builds the palette's filtered heading/row model without depending on egui.
///
/// Categories omitted from `PALETTE_CATEGORY_ORDER` are appended in enum order.
/// That deterministic fallback prevents a newly enabled category from silently
/// disappearing; the catalog coverage test additionally requires product code
/// to assign every visible category an intentional position in the constant.
pub fn group_visible_descriptors(query: &str) -> Vec<(ActionCategory, Vec<ActionDescriptor>)> {
    let mut groups: Vec<(ActionCategory, Vec<ActionDescriptor>)> = Vec::new();

    for descriptor in visible_descriptors().filter(|descriptor| matches(descriptor, query)) {
        if let Some((_, rows)) = groups
            .iter_mut()
            .find(|(category, _)| *category == descriptor.category)
        {
            rows.push(descriptor);
        } else {
            groups.push((descriptor.category, vec![descriptor]));
        }
    }

    groups.sort_by_key(|(category, _)| {
        PALETTE_CATEGORY_ORDER
            .iter()
            .position(|ordered| ordered == category)
            .map_or((1, *category as usize), |position| (0, position))
    });
    groups
}

pub fn matches(d: &ActionDescriptor, q: &str) -> bool {
    let q = q.to_lowercase();
    q.is_empty()
        || d.name.to_lowercase().contains(&q)
        || d.description.to_lowercase().contains(&q)
        || d.category.label().to_lowercase().contains(&q)
        || d.keywords.iter().any(|k| k.contains(&q))
}
pub fn action_name(a: &MkAction) -> &'static str {
    match a {
        MkAction::KeyDown(_) => "Key Down",
        MkAction::KeyUp(_) => "Key Up",
        MkAction::KeyPress(_) => "Key Press",
        MkAction::Hotkey(_) => "Hotkey",
        MkAction::Text(_) => "Text",
        MkAction::Notify(_) => "Notify",
        MkAction::PlaySound(_) => "Play Sound",
        MkAction::MouseMove(_) => "Mouse Move",
        MkAction::MouseDrag(_) => "Mouse Drag",
        MkAction::MouseClick(_) => "Mouse Click",
        MkAction::MouseDown(_) => "Mouse Down",
        MkAction::MouseUp(_) => "Mouse Up",
        MkAction::MouseScroll { .. } => "Mouse Scroll",
        MkAction::Delay { .. } => "Delay",
        MkAction::Process(_) => "Run Program",
        MkAction::LauncherCommand { .. } => "Launcher Command",
        MkAction::WindowActivate(_) => "Activate Window",
        MkAction::WindowClose(_) => "Close Window",
        MkAction::WindowWait(_) => "Wait for Window",
        MkAction::WindowMoveResize(_) => "Move / Resize Window",
        MkAction::WindowState {
            state: MkWindowState::Minimize,
            ..
        } => "Minimize Window",
        MkAction::WindowState {
            state: MkWindowState::Maximize,
            ..
        } => "Maximize Window",
        MkAction::WindowState {
            state: MkWindowState::Restore,
            ..
        } => "Restore Window",
        MkAction::VirtualDesktop(MkVirtualDesktopAction::Create) => "Create Virtual Desktop",
        MkAction::VirtualDesktop(MkVirtualDesktopAction::SwitchLeft) => {
            "Switch Virtual Desktop Left"
        }
        MkAction::VirtualDesktop(MkVirtualDesktopAction::SwitchRight) => {
            "Switch Virtual Desktop Right"
        }
        MkAction::VirtualDesktop(MkVirtualDesktopAction::CloseCurrent) => {
            "Close Current Virtual Desktop"
        }
        MkAction::WaitUntil { .. } => "Wait Until",
        MkAction::SetVariable { .. } => "Set Variable",
        MkAction::UnsetVariable { .. } => "Unset Variable",
        MkAction::PromptInput(_) => "Prompt for Input",
        MkAction::If(_) => "If",
        MkAction::Else => "Else",
        MkAction::EndIf => "End If",
        MkAction::RepeatStart { .. } => "Repeat",
        MkAction::RepeatEnd => "End Repeat",
        MkAction::WhileStart { .. } => "While",
        MkAction::WhileEnd => "End While",
        MkAction::Break => "Break",
        MkAction::Continue => "Continue",
        MkAction::ImageFind(_) => "Find Image",
        MkAction::FindPixel(_) => "Find Pixel Color",
        MkAction::ImageClick(_) => "Click Image",
        MkAction::PixelCheck { .. } => "Check Pixel Color",
        MkAction::CaptureScreenshot(_) => "Capture Screenshot",
        MkAction::WaitForVisualChange(_) => "Wait for Visual Change",
        MkAction::UiInvoke(_)
        | MkAction::UiSetValue { .. }
        | MkAction::UiReadValue { .. }
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => "UI Automation — currently unavailable",
    }
}
fn mouse(b: &MkMouseButton) -> &'static str {
    match b {
        MkMouseButton::Left => "Left",
        MkMouseButton::Right => "Right",
        MkMouseButton::Middle => "Middle",
        MkMouseButton::X1 => "X1",
        MkMouseButton::X2 => "X2",
    }
}
pub fn action_details(a: &MkAction) -> String {
    action_details_core(a, None, &[])
}
pub fn action_details_with_asset_name(a: &MkAction, asset_name: Option<&str>) -> String {
    action_details_core(a, asset_name, &[])
}
pub fn action_details_with_assets(a: &MkAction, assets: &[MkImageAsset]) -> String {
    action_details_core(a, None, assets)
}
fn action_details_core(a: &MkAction, asset_name: Option<&str>, assets: &[MkImageAsset]) -> String {
    match a {
        MkAction::KeyDown(k) | MkAction::KeyUp(k) | MkAction::KeyPress(k) => {
            super::key_capture::key_name(k)
        }
        MkAction::Hotkey(k) => k
            .iter()
            .map(super::key_capture::key_name)
            .collect::<Vec<_>>()
            .join(" + "),
        MkAction::Text(p) => format!(
            "{} {} characters",
            match p.mode {
                MkTextMode::Type => "Type",
                MkTextMode::Paste => "Paste",
            },
            p.text.chars().count()
        ),
        MkAction::Notify(p) => format!(
            "{} · {}",
            p.kind.label(),
            if p.title.trim().is_empty() {
                "Untitled notification"
            } else {
                &p.title
            }
        ),
        MkAction::PlaySound(p) => p.sound.clone(),
        MkAction::MouseClick(p) => format!(
            "{} ×{} @ {}",
            mouse(&p.button),
            p.clicks,
            format_coordinate_target_with_assets(&p.target, assets)
        ),
        MkAction::MouseDown(b) | MkAction::MouseUp(b) => mouse(b).into(),
        MkAction::MouseScroll { axis, i32_delta } => {
            let direction = match (axis, i32_delta.is_negative()) {
                (MkMouseScrollAxis::Vertical, false) => "Vertical Up",
                (MkMouseScrollAxis::Vertical, true) => "Vertical Down",
                (MkMouseScrollAxis::Horizontal, false) => "Horizontal Right",
                (MkMouseScrollAxis::Horizontal, true) => "Horizontal Left",
            };
            if *i32_delta % 120 == 0 {
                format!(
                    "{direction} · {} notch(es) · {i32_delta} wheel units",
                    (i32_delta / 120).unsigned_abs()
                )
            } else {
                format!(
                    "{direction} · raw delta {} wheel units",
                    i32_delta.unsigned_abs()
                )
            }
        }
        MkAction::Delay { milliseconds } => format!("{milliseconds} ms"),
        MkAction::Process(p) => format!("{} {}", p.program, p.arguments.join(" ")),
        MkAction::LauncherCommand { command, args } => {
            format!("{} {}", command, args.as_deref().unwrap_or(""))
        }
        MkAction::SetVariable { name, .. } => format!("Set {name}"),
        MkAction::UnsetVariable { name } => format!("Unset {name}"),
        MkAction::PromptInput(p) => format!(
            "Ask ‘{}’ → {}{}",
            p.prompt,
            p.variable,
            if p.copy_to_clipboard {
                " · copy to clipboard"
            } else {
                ""
            }
        ),
        MkAction::RepeatStart { count } => format!("{count} times"),
        MkAction::ImageFind(p) => format_image_details(p, asset_name, assets, false),
        MkAction::ImageClick(p) => format_image_details(p, asset_name, assets, true),
        MkAction::FindPixel(p) => format!(
            "{} ±{} · {} · {}",
            p.color,
            p.tolerance,
            region_summary(&p.region),
            match p.not_found_policy {
                MkImageNotFoundPolicy::Continue => "continue if missing",
                MkImageNotFoundPolicy::Fail => "fail if missing",
            }
        ),
        MkAction::PixelCheck {
            target,
            color,
            tolerance,
        } => format!(
            "{color} ±{tolerance} @ {}",
            format_coordinate_target_with_assets(target, assets)
        ),
        MkAction::CaptureScreenshot(p) => {
            let destination = match p.destination {
                MkScreenshotDestination::File => "File",
                MkScreenshotDestination::Clipboard => "Clipboard",
                MkScreenshotDestination::Both => "File + Clipboard",
            };
            let mut summary = region_summary(&p.region);
            if let Some(path) = &p.path {
                summary.push_str(&format!(" → {path}"));
            }
            format!("{summary} · {destination}")
        }
        MkAction::WaitForVisualChange(p) => format!(
            "{} changes ≥ {}% · timeout {} ms",
            region_summary(&p.region),
            p.change_threshold_percent,
            p.timeout_ms
        ),
        MkAction::UiSetValue { value, .. } => {
            format!("Unavailable UI Automation action (set value to {value})")
        }
        MkAction::UiReadValue { variable, .. } => {
            format!("Unavailable UI Automation action (read into {variable})")
        }
        MkAction::MouseMove(p) => format!(
            "{} · {}",
            format_coordinate_target_with_assets(&p.target, assets),
            if p.duration_ms == 0 {
                "Instant".into()
            } else {
                format!("Smooth {} ms", p.duration_ms)
            }
        ),
        MkAction::MouseDrag(p) => format!(
            "{} → {} · {} · {} ms",
            format_coordinate_target_with_assets(&p.from, assets),
            format_coordinate_target_with_assets(&p.to, assets),
            mouse(&p.button),
            p.duration_ms
        ),
        MkAction::WindowActivate(p) | MkAction::WindowWait(p) => {
            format!("Window {}", p.matcher.title.as_deref().unwrap_or("match"))
        }
        MkAction::WindowClose(p) => format!("Window {}", p.title.as_deref().unwrap_or("match")),
        MkAction::WindowMoveResize(p) => {
            let mut operations = Vec::new();
            if let (Some(x), Some(y)) = (p.x, p.y) {
                operations.push(format!("Move to ({x}, {y})"));
            }
            if let (Some(w), Some(h)) = (p.width, p.height) {
                operations.push(format!("Resize to {w} × {h}"));
            }
            format!(
                "{} · Window {}",
                operations.join(" + "),
                p.matcher.title.as_deref().unwrap_or("match")
            )
        }
        MkAction::WindowState { matcher, state } => format!(
            "{:?} · Window {}",
            state,
            matcher.title.as_deref().unwrap_or("match")
        ),
        MkAction::VirtualDesktop(MkVirtualDesktopAction::Create) => {
            "Create a new virtual desktop".into()
        }
        MkAction::VirtualDesktop(MkVirtualDesktopAction::SwitchLeft) => {
            "Switch virtual desktop left".into()
        }
        MkAction::VirtualDesktop(MkVirtualDesktopAction::SwitchRight) => {
            "Switch virtual desktop right".into()
        }
        MkAction::VirtualDesktop(MkVirtualDesktopAction::CloseCurrent) => {
            "Close the current virtual desktop using native Windows behavior".into()
        }
        MkAction::WaitUntil { condition, wait } => {
            format_wait_until(condition, wait, asset_name, assets)
        }
        MkAction::If(condition) | MkAction::WhileStart { condition } => {
            condition_summary(condition, asset_name, assets)
        }
        MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatEnd
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue => "Structural control marker".into(),
        MkAction::UiInvoke(_)
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => "Unavailable UI Automation action (saved target preserved)".into(),
    }
}

fn matcher_summary(m: &MkWindowMatcher) -> String {
    [
        m.process.as_deref(),
        m.title.as_deref(),
        m.title_regex.as_deref(),
        m.class.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" / ")
}
fn asset_display_name(id: u64, preferred: Option<&str>, assets: &[MkImageAsset]) -> String {
    assets
        .iter()
        .find(|asset| asset.id == id)
        .and_then(|asset| {
            let name = asset.name.trim();
            if !name.is_empty() {
                Some(name.to_owned())
            } else {
                std::path::Path::new(&asset.relative_path)
                    .file_name()?
                    .to_str()
                    .map(str::to_owned)
            }
        })
        .or_else(|| {
            preferred.filter(|s| !s.trim().is_empty()).map(|s| {
                std::path::Path::new(s)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(s)
                    .to_owned()
            })
        })
        .unwrap_or_else(|| format!("Missing image #{id}"))
}
pub(super) fn region_summary(region: &SearchRegion) -> String {
    match region {
        SearchRegion::Desktop => "Entire Desktop".into(),
        SearchRegion::Monitor { index } => format!("Monitor {index}"),
        SearchRegion::Rectangle { rect } => format!(
            "Rectangle ({},{}) {}×{}",
            rect.x, rect.y, rect.width, rect.height
        ),
        SearchRegion::Window { matcher } => format!("Window: {}", matcher_summary(matcher)),
        SearchRegion::ClientArea { matcher } => format!("Client: {}", matcher_summary(matcher)),
    }
}
fn condition_region_summary(region: &SearchRegion) -> String {
    match region {
        SearchRegion::Rectangle { rect } => format!("Rectangle {}×{}", rect.width, rect.height),
        other => region_summary(other),
    }
}
fn condition_summary(c: &MkCondition, preferred: Option<&str>, assets: &[MkImageAsset]) -> String {
    match c {
        MkCondition::ImageSearch { search, found } => format!(
            "Image currently {}: {} · {}",
            if *found { "visible" } else { "not visible" },
            asset_display_name(search.asset_id, preferred, assets),
            condition_region_summary(&search.region)
        ),
        MkCondition::PreviousImageResult { asset_id, found } => format!(
            "Previous image search: {} = {}",
            asset_id
                .map(|id| asset_display_name(id, preferred, assets))
                .unwrap_or_else(|| "any image".into()),
            if *found { "Found" } else { "Not Found" }
        ),
        MkCondition::All { conditions } => format!(
            "All ({})",
            conditions
                .iter()
                .map(|c| condition_summary(c, preferred, assets))
                .collect::<Vec<_>>()
                .join("; ")
        ),
        MkCondition::Any { conditions } => format!(
            "Any ({})",
            conditions
                .iter()
                .map(|c| condition_summary(c, preferred, assets))
                .collect::<Vec<_>>()
                .join("; ")
        ),
        MkCondition::Not { condition } => {
            format!("Not ({})", condition_summary(condition, preferred, assets))
        }
        MkCondition::WindowExists { matcher } => {
            format!("Window exists: {}", matcher_summary(matcher))
        }
        MkCondition::WindowActive { matcher } => {
            format!("Window active: {}", matcher_summary(matcher))
        }
        MkCondition::Variable { name, op, value } => format!("{name} {op:?} {value:?}"),
        MkCondition::PixelResult {
            target,
            color,
            tolerance,
        } => format!(
            "Pixel {color} ±{tolerance} @ {}",
            format_coordinate_target_with_assets(target, assets)
        ),
    }
}
fn format_wait_until(
    c: &MkCondition,
    wait: &MkWaitOptions,
    preferred: Option<&str>,
    assets: &[MkImageAsset],
) -> String {
    if let MkCondition::ImageSearch { search, found } = c {
        let mut parts = vec![format!(
            "{} {}",
            asset_display_name(search.asset_id, preferred, assets),
            if *found { "appears" } else { "disappears" }
        )];
        if !matches!(search.region, SearchRegion::Desktop) {
            parts.push(condition_region_summary(&search.region));
        }
        parts.push(format!("timeout {} ms", wait.timeout_ms));
        if wait.poll_interval_ms != 100 {
            parts.push(format!("poll every {} ms", wait.poll_interval_ms));
        }
        parts.join(" · ")
    } else {
        format!(
            "{} · timeout {} ms",
            condition_summary(c, preferred, assets),
            wait.timeout_ms
        )
    }
}
fn format_image_details(
    p: &MkImagePayload,
    asset_name: Option<&str>,
    assets: &[MkImageAsset],
    click: bool,
) -> String {
    let image = asset_display_name(p.asset_id, asset_name, assets);
    let mut parts = vec![image, region_summary(&p.region)];
    if !click && p.tolerance != 0 {
        parts.push(format!("tolerance {}", p.tolerance));
    }
    if click {
        parts.push(match p.return_point {
            ReturnPoint::Center => "center".into(),
            ReturnPoint::TopLeft => "top-left".into(),
        });
    }
    if click {
        parts.push(format!("{} ms", p.wait.timeout_ms));
    }
    if !click {
        parts.push(match p.not_found_policy {
            MkImageNotFoundPolicy::Continue => "continue if missing".into(),
            MkImageNotFoundPolicy::Fail => "fail if missing".into(),
        });
        let outputs = [
            ("found", &p.outputs.found),
            ("point", &p.outputs.point),
            ("x", &p.outputs.x),
            ("y", &p.outputs.y),
        ]
        .into_iter()
        .filter_map(|(slot, name)| name.as_deref().map(|name| format!("{slot}→{name}")))
        .collect::<Vec<_>>();
        if !outputs.is_empty() {
            parts.push(outputs.join(", "));
        }
    }
    parts.join(" · ")
}

#[cfg(test)]
mod grouping_tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn source_visual_descriptors_are_not_contiguous() {
        let categories: Vec<_> = descriptors()
            .into_iter()
            .map(|descriptor| descriptor.category)
            .collect();
        let visual_indices: Vec<_> = categories
            .iter()
            .enumerate()
            .filter_map(|(index, category)| (*category == ActionCategory::Visual).then_some(index))
            .collect();

        assert!(visual_indices.windows(2).any(|indices| {
            categories[indices[0] + 1..indices[1]]
                .iter()
                .any(|category| *category != ActionCategory::Visual)
        }));
    }

    #[test]
    fn empty_query_has_one_complete_visual_group() {
        let groups = group_visible_descriptors("");
        let visual_groups: Vec<_> = groups
            .iter()
            .filter(|(category, _)| *category == ActionCategory::Visual)
            .collect();
        assert_eq!(visual_groups.len(), 1);

        let expected: Vec<_> = visible_descriptors()
            .filter(|descriptor| descriptor.category == ActionCategory::Visual)
            .map(|descriptor| descriptor.name)
            .collect();
        let actual: Vec<_> = visual_groups[0]
            .1
            .iter()
            .map(|descriptor| descriptor.name)
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn image_search_emits_only_nonempty_matching_groups() {
        let groups = group_visible_descriptors("ImAgE");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, ActionCategory::Visual);
        assert!(!groups[0].1.is_empty());
        assert!(
            groups[0]
                .1
                .iter()
                .all(|descriptor| matches(descriptor, "image"))
        );
        for expected in [
            "Wait for Image",
            "Wait for Image to Disappear",
            "Find Image",
            "Click Image",
        ] {
            assert!(
                groups[0]
                    .1
                    .iter()
                    .any(|descriptor| descriptor.name == expected)
            );
        }
    }

    #[test]
    fn search_preserves_description_category_and_keyword_fields() {
        let description = group_visible_descriptors("percentage of PIXELS");
        assert!(description.iter().any(|(_, rows)| {
            rows.iter()
                .any(|descriptor| descriptor.name == "Wait for Visual Change")
        }));

        let category = group_visible_descriptors("keyboard & TEXT");
        assert!(!category.is_empty());
        assert!(category.iter().all(|(kind, rows)| {
            *kind == ActionCategory::KeyboardText
                && rows
                    .iter()
                    .all(|descriptor| matches(descriptor, "keyboard & text"))
        }));

        let keyword = group_visible_descriptors("CLIPBOARD");
        assert!(keyword.iter().any(|(_, rows)| {
            rows.iter()
                .any(|descriptor| descriptor.name == "Capture Screenshot")
        }));
    }

    #[test]
    fn groups_are_unique_nonempty_ordered_and_stable() {
        let first = group_visible_descriptors("");
        let second = group_visible_descriptors("");
        let categories: Vec<_> = first.iter().map(|(category, _)| *category).collect();
        let unique: BTreeSet<_> = categories.iter().copied().collect();

        assert!(first.iter().all(|(_, rows)| !rows.is_empty()));
        assert_eq!(unique.len(), categories.len());
        let expected_order: Vec<_> = PALETTE_CATEGORY_ORDER
            .iter()
            .copied()
            .filter(|category| categories.contains(category))
            .collect();
        assert_eq!(categories, expected_order);
        assert_eq!(
            first
                .iter()
                .map(|(category, rows)| (
                    *category,
                    rows.iter().map(|row| row.name).collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>(),
            second
                .iter()
                .map(|(category, rows)| (
                    *category,
                    rows.iter().map(|row| row.name).collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn disabled_ui_automation_has_neither_rows_nor_heading() {
        assert!(
            descriptors()
                .iter()
                .any(|descriptor| descriptor.category == ActionCategory::UiAutomation)
        );
        assert!(
            group_visible_descriptors("")
                .iter()
                .all(|(category, _)| *category != ActionCategory::UiAutomation)
        );
    }

    #[test]
    fn every_visible_category_has_an_explicit_product_position() {
        let visible_categories: BTreeSet<_> = visible_descriptors()
            .map(|descriptor| descriptor.category)
            .collect();
        for category in visible_categories {
            assert!(
                PALETTE_CATEGORY_ORDER.contains(&category),
                "visible category {category:?} is missing from PALETTE_CATEGORY_ORDER"
            );
        }
    }
}

#[cfg(test)]
mod paste_tests {
    use super::*;

    #[test]
    fn text_details_distinguish_type_and_paste() {
        let action = |mode| {
            MkAction::Text(MkTextPayload {
                text: "abc".into(),
                mode,
            })
        };
        assert_eq!(
            action_details(&action(MkTextMode::Type)),
            "Type 3 characters"
        );
        assert_eq!(
            action_details(&action(MkTextMode::Paste)),
            "Paste 3 characters"
        );
        assert!(!action_details(&action(MkTextMode::Paste)).contains("Coming"));
    }

    #[test]
    fn horizontal_scroll_details_include_axis_direction_and_magnitude() {
        let details = action_details(&MkAction::MouseScroll {
            axis: MkMouseScrollAxis::Horizontal,
            i32_delta: -240,
        });
        assert!(details.contains("Horizontal Left"));
        assert!(details.contains("2 notch(es)"));
        assert!(details.contains("-240 wheel units"));
    }

    #[test]
    fn notification_and_sound_actions_have_dedicated_catalog_presentation() {
        let notify = MkAction::Notify(MkNotifyPayload {
            title: "Build complete".into(),
            kind: MkNotificationKind::Success,
            duration: MkNotificationDuration::Long,
            ..MkNotifyPayload::default()
        });
        assert_eq!(editor_for_action(&notify), EditorKind::Notify);
        assert_eq!(action_name(&notify), "Notify");
        assert_eq!(action_details(&notify), "Success · Build complete");

        let sound = MkAction::PlaySound(MkPlaySoundPayload {
            sound: "ReminderStart.wav".into(),
        });
        assert_eq!(editor_for_action(&sound), EditorKind::PlaySound);
        assert_eq!(action_name(&sound), "Play Sound");
        assert_eq!(action_details(&sound), "ReminderStart.wav");
    }

    #[test]
    fn notifications_category_and_descriptors_are_explicit_and_searchable() {
        assert_eq!(
            PALETTE_CATEGORY_ORDER
                .iter()
                .filter(|c| **c == ActionCategory::Notifications)
                .count(),
            1
        );
        let rows: Vec<_> = descriptors()
            .into_iter()
            .filter(|d| d.category == ActionCategory::Notifications)
            .collect();
        assert_eq!(
            rows.iter().map(|d| d.name).collect::<Vec<_>>(),
            ["Notify", "Play Sound"]
        );
        assert!(
            matches!((rows[0].make_default)(), MkAction::Notify(p) if p == MkNotifyPayload::default())
        );
        assert!(
            matches!((rows[1].make_default)(), MkAction::PlaySound(p) if p == MkPlaySoundPayload::default())
        );
        for query in ["toast", "notification"] {
            assert_eq!(
                rows.iter()
                    .filter(|d| matches(d, query))
                    .map(|d| d.name)
                    .collect::<Vec<_>>(),
                ["Notify"]
            );
        }
        assert_eq!(
            rows.iter()
                .filter(|d| matches(d, "audio"))
                .map(|d| d.name)
                .collect::<Vec<_>>(),
            ["Play Sound"]
        );
        assert_eq!(
            rows[0].runtime == RuntimeAvailability::Supported,
            crate::mkmacro::executor::has_runtime_support(&(rows[0].make_default)())
        );
        assert_eq!(
            rows[1].runtime == RuntimeAvailability::Supported,
            crate::mkmacro::executor::has_runtime_support(&(rows[1].make_default)())
        );
    }

    #[test]
    fn image_wait_presets_are_searchable_configurable_wait_until_actions() {
        let standard_wait = wait();
        for (name, expected_found) in [
            ("Wait for Image", true),
            ("Wait for Image to Disappear", false),
        ] {
            let descriptor = visible_descriptors()
                .find(|descriptor| descriptor.name == name)
                .unwrap_or_else(|| panic!("missing visible descriptor {name}"));
            assert_eq!(descriptor.category, ActionCategory::Visual);
            assert_eq!(descriptor.editor, EditorKind::Condition);
            assert_eq!(descriptor.runtime, RuntimeAvailability::Supported);
            assert!(matches!(
                descriptor.editor.contract(),
                Some(EditorContract::Configurable { field_count: 1.. })
            ));
            for keyword in [
                "wait",
                "image",
                "visual",
                "appear",
                "visible",
                "disappear",
                "gone",
            ] {
                assert!(
                    matches(&descriptor, keyword),
                    "{name} did not match {keyword}"
                );
            }

            let action = (descriptor.make_default)();
            assert!(editor_route_recognizes(&action, EditorKind::Condition));
            let MkAction::WaitUntil { condition, wait } = action else {
                panic!("{name} did not create WaitUntil")
            };
            assert_eq!(wait, standard_wait);
            assert_eq!(
                condition,
                image_cond(expected_found),
                "{name} image-search defaults"
            );
        }
    }

    #[test]
    fn image_wait_details_distinguish_expected_result_and_asset() {
        let visible = MkAction::WaitUntil {
            condition: image_cond(true),
            wait: wait(),
        };
        let absent = MkAction::WaitUntil {
            condition: image_cond(false),
            wait: wait(),
        };
        assert!(
            action_details_with_asset_name(&visible, Some("button.png"))
                .contains("button.png appears · timeout 5000 ms")
        );
        assert!(
            action_details_with_asset_name(&absent, Some("button.png"))
                .contains("button.png disappears · timeout 5000 ms")
        );
    }

    #[test]
    fn image_summaries_use_friendly_names_and_exact_intent() {
        let assets = [MkImageAsset {
            id: 7,
            name: "save.png".into(),
            relative_path: "images/original.png".into(),
        }];
        let mut payload = ip(MkImageNotFoundPolicy::Continue);
        payload.asset_id = 7;
        payload.region = SearchRegion::Window {
            matcher: MkWindowMatcher {
                process: Some("explorer.exe".into()),
                ..Default::default()
            },
        };
        assert_eq!(
            action_details_with_assets(&MkAction::ImageFind(payload), &assets),
            "save.png · Window: explorer.exe · continue if missing"
        );

        let target = MkCoordinateTarget::Image {
            asset_id: 7,
            offset: MkPoint { x: 0, y: 0 },
        };
        assert_eq!(
            format_coordinate_target_with_assets(&target, &assets),
            "Image Result: save.png + (0,0)"
        );
        assert_eq!(
            action_details_with_assets(
                &MkAction::MouseMove(MkMouseMovePayload {
                    target,
                    duration_ms: 500
                }),
                &assets
            ),
            "Image Result: save.png + (0,0) · Smooth 500 ms"
        );
    }

    #[test]
    fn recursive_image_conditions_keep_live_and_previous_results_distinct() {
        let assets = [MkImageAsset {
            id: 7,
            name: "save.png".into(),
            relative_path: String::new(),
        }];
        let live = MkCondition::ImageSearch {
            search: MkImageSearchCondition {
                asset_id: 7,
                region: SearchRegion::Rectangle {
                    rect: ScreenRect {
                        x: 10,
                        y: 20,
                        width: 800,
                        height: 500,
                    },
                },
                tolerance: 0,
                alpha: AlphaPolicy::Compare,
                return_point: ReturnPoint::Center,
            },
            found: true,
        };
        assert_eq!(
            condition_summary(&live, None, &assets),
            "Image currently visible: save.png · Rectangle 800×500"
        );
        let nested = MkCondition::All {
            conditions: vec![
                live,
                MkCondition::Not {
                    condition: Box::new(MkCondition::PreviousImageResult {
                        asset_id: Some(7),
                        found: false,
                    }),
                },
            ],
        };
        assert_eq!(
            condition_summary(&nested, None, &assets),
            "All (Image currently visible: save.png · Rectangle 800×500; Not (Previous image search: save.png = Not Found))"
        );
        assert_eq!(asset_display_name(9, None, &assets), "Missing image #9");
    }
}
pub fn format_coordinate_target(target: &MkCoordinateTarget) -> String {
    format_coordinate_target_with_assets(target, &[])
}
pub fn format_coordinate_target_with_assets(
    target: &MkCoordinateTarget,
    assets: &[MkImageAsset],
) -> String {
    match target {
        MkCoordinateTarget::Screen { point } => format!("Screen ({}, {})", point.x, point.y),
        MkCoordinateTarget::ActiveWindow { point } => {
            format!("Active Window ({}, {})", point.x, point.y)
        }
        MkCoordinateTarget::WindowClient { matcher, point } => {
            let identity = matcher
                .process
                .as_deref()
                .or(matcher.title.as_deref())
                .or(matcher.title_regex.as_deref())
                .or(matcher.class.as_deref())
                .unwrap_or("unconfigured window");
            format!("Matched Window {identity} ({}, {})", point.x, point.y)
        }
        MkCoordinateTarget::Variable { name } => format!("Variable <{name}>"),
        MkCoordinateTarget::Image { asset_id, offset } => {
            format!(
                "Image Result: {} + ({},{})",
                asset_display_name(*asset_id, None, assets),
                offset.x,
                offset.y
            )
        }
        MkCoordinateTarget::Pixel { search_id, offset } => {
            format!("Pixel Result: #{search_id} + ({},{})", offset.x, offset.y)
        }
    }
}
pub fn action_depths(m: &MkMacro) -> Vec<usize> {
    if let Ok(p) = crate::mkmacro::compile(m) {
        return p.instructions.iter().map(|i| i.depth).collect();
    }
    let mut n: usize = 0;
    let mut out = vec![];
    for s in &m.steps {
        if matches!(
            s.action,
            MkAction::Else | MkAction::EndIf | MkAction::RepeatEnd | MkAction::WhileEnd
        ) {
            n = n.saturating_sub(1)
        }
        out.push(n);
        if matches!(
            s.action,
            MkAction::If(_)
                | MkAction::Else
                | MkAction::RepeatStart { .. }
                | MkAction::WhileStart { .. }
        ) {
            n += 1
        }
    }
    out
}
fn step(action: MkAction) -> MkStep {
    MkStep {
        id: 0,
        enabled: true,
        repeat: 1,
        delay_after_ms: 0,
        on_error: Default::default(),
        action,
    }
}
pub fn insert_action(d: &mut MkMacroDialog, action: MkAction) {
    if matches!(
        action,
        MkAction::If(_) | MkAction::RepeatStart { .. } | MkAction::WhileStart { .. }
    ) {
        let ids: Vec<_> = d
            .selected_macro()
            .into_iter()
            .flat_map(|m| m.steps.iter())
            .filter(|s| d.selection.ids.contains(&s.id))
            .map(|s| s.id)
            .collect();
        let intent = if ids.is_empty() {
            super::action_editor::InsertionIntent::Plain {
                after_step_id: None,
            }
        } else {
            super::action_editor::InsertionIntent::Wrap { step_ids: ids }
        };
        if let Err(error) = apply_structural(d, step(action), intent) {
            d.command_error = Some(error);
        }
        return;
    }
    if let Err(error) = insert_direct(d, action) {
        d.command_error = Some(error);
    }
}

fn insertion_position(d: &MkMacroDialog) -> usize {
    let ids = &d.selection.ids;
    d.selected_macro().map_or(0, |m| {
        m.steps
            .iter()
            .rposition(|s| ids.contains(&s.id))
            .map_or(m.steps.len(), |i| i + 1)
    })
}

fn insert_direct(d: &mut MkMacroDialog, action: MkAction) -> Result<u64, String> {
    if matches!(
        action,
        MkAction::If(_) | MkAction::RepeatStart { .. } | MkAction::WhileStart { .. }
    ) {
        return Err("Block openers must be configured before insertion".into());
    }
    let pos = insertion_position(d);
    if matches!(
        action,
        MkAction::Else
            | MkAction::EndIf
            | MkAction::RepeatEnd
            | MkAction::WhileEnd
            | MkAction::Break
            | MkAction::Continue
    ) {
        validate_direct_context(
            d.selected_macro().ok_or("No macro is selected")?,
            pos,
            &action,
        )?;
    }
    let m = d.selected_macro_mut().ok_or("No macro is selected")?;
    m.steps.insert(pos, step(action));
    repair_ids(&mut d.draft);
    let id = d.selected_macro().unwrap().steps[pos].id;
    d.selection.ids.clear();
    d.selection.ids.insert(id);
    d.command_error = None;
    d.mark_dirty();
    Ok(id)
}

fn validate_direct_context(m: &MkMacro, pos: usize, action: &MkAction) -> Result<(), String> {
    #[derive(Clone, Copy)]
    enum Open {
        If(bool),
        Repeat,
        While,
    }
    let mut stack = Vec::new();
    for s in &m.steps[..pos] {
        match s.action {
            MkAction::If(_) => stack.push(Open::If(false)),
            MkAction::RepeatStart { .. } => stack.push(Open::Repeat),
            MkAction::WhileStart { .. } => stack.push(Open::While),
            MkAction::Else => {
                if let Some(Open::If(seen)) = stack.last_mut() {
                    *seen = true
                }
            }
            MkAction::EndIf | MkAction::RepeatEnd | MkAction::WhileEnd => {
                stack.pop();
            }
            _ => {}
        }
    }
    let bad = match action {
        MkAction::Else => !matches!(stack.last(), Some(Open::If(false))),
        MkAction::EndIf => !matches!(stack.last(), Some(Open::If(_))),
        MkAction::RepeatEnd => !matches!(stack.last(), Some(Open::Repeat)),
        MkAction::WhileEnd => !matches!(stack.last(), Some(Open::While)),
        MkAction::Break | MkAction::Continue => !stack
            .iter()
            .any(|x| matches!(x, Open::Repeat | Open::While)),
        _ => false,
    };
    if bad {
        Err(format!(
            "{} is not valid at the current insertion position",
            action_name(action)
        ))
    } else {
        Ok(())
    }
}

pub fn apply_structural(
    d: &mut MkMacroDialog,
    mut opener: MkStep,
    intent: super::action_editor::InsertionIntent,
) -> Result<u64, String> {
    use super::action_editor::InsertionIntent;
    let terminator = match opener.action {
        MkAction::If(_) => MkAction::EndIf,
        MkAction::RepeatStart { .. } => MkAction::RepeatEnd,
        MkAction::WhileStart { .. } => MkAction::WhileEnd,
        _ => return Err("Action is not a block opener".into()),
    };
    let original = d.selected_macro().ok_or("No macro is selected")?;
    let (first, last) = match intent {
        InsertionIntent::Plain { after_step_id } => {
            let p = after_step_id
                .and_then(|id| original.steps.iter().position(|s| s.id == id))
                .map_or(original.steps.len(), |i| i + 1);
            (p, p)
        }
        InsertionIntent::Wrap { step_ids } => {
            let indices: Vec<_> = original
                .steps
                .iter()
                .enumerate()
                .filter_map(|(i, s)| step_ids.contains(&s.id).then_some(i))
                .collect();
            if indices.len() != step_ids.len()
                || indices.is_empty()
                || indices.windows(2).any(|w| w[1] != w[0] + 1)
            {
                return Err("Block wrapping requires one contiguous selection".into());
            }
            (*indices.first().unwrap(), indices.last().unwrap() + 1)
        }
        InsertionIntent::EditExisting { .. } => {
            return Err("Existing rows cannot be inserted as new blocks".into());
        }
    };
    let next_id = d
        .draft
        .macros
        .iter()
        .flat_map(|m| m.steps.iter())
        .map(|s| s.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    opener.id = next_id;
    let mut candidate = original.clone();
    candidate.steps.insert(first, opener);
    let mut ending = step(terminator);
    ending.id = next_id.saturating_add(1);
    candidate.steps.insert(last + 1, ending);
    if crate::mkmacro::compile(&candidate).is_err() {
        return Err("The selection cuts through an existing block boundary".into());
    }
    let open_id = candidate.steps[first].id;
    let end_id = candidate.steps[last + 1].id;
    *d.selected_macro_mut().unwrap() = candidate;
    // Deterministically select both newly-created boundary rows.
    d.selection.ids.clear();
    d.selection.ids.insert(open_id);
    d.selection.ids.insert(end_id);
    d.command_error = None;
    d.mark_dirty();
    Ok(open_id)
}

/// Select a catalog entry without ever replacing an in-progress transaction.
pub fn select_descriptor(d: &mut MkMacroDialog, descriptor: &ActionDescriptor) -> bool {
    // Selection is an authoring capability, not a generic model-loading route.
    // Hidden actions remain deserializable but cannot bypass the catalog filter
    // through a direct call to this function.
    if !is_available_in_palette(descriptor) || d.action_editor.draft.is_some() {
        return false;
    }
    let action = (descriptor.make_default)();
    assert!(
        editor_route_recognizes(&action, descriptor.editor),
        "catalog editor/action mismatch for {}",
        descriptor.name
    );
    match descriptor.editor {
        EditorKind::DirectInsert => insert_action(d, action),
        kind => {
            let ids: Vec<_> = d
                .selected_macro()
                .into_iter()
                .flat_map(|m| m.steps.iter())
                .filter(|s| d.selection.ids.contains(&s.id))
                .map(|s| s.id)
                .collect();
            let after = ids.last().copied();
            let structural = matches!(
                action,
                MkAction::If(_) | MkAction::RepeatStart { .. } | MkAction::WhileStart { .. }
            );
            d.action_editor.begin_new_with_editor(action, kind);
            d.action_editor.insertion = Some(if structural && !ids.is_empty() {
                super::action_editor::InsertionIntent::Wrap { step_ids: ids }
            } else {
                super::action_editor::InsertionIntent::Plain {
                    after_step_id: after,
                }
            });
        }
    }
    true
}

pub fn close(d: &mut MkMacroDialog) {
    d.action_catalog_visible = false;
}

pub(super) fn show_modal(ctx: &egui::Context, d: &mut MkMacroDialog) {
    if !d.action_catalog_visible {
        return;
    }
    let mut open = d.action_catalog_visible;
    let mut close_clicked = false;
    egui::Window::new("Add Action")
        .open(&mut open)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.add(egui::TextEdit::singleline(&mut d.action_search).hint_text("Search actions"));
            egui::ScrollArea::vertical()
                .max_height(430.0)
                .show(ui, |ui| {
                    let query = d.action_search.clone();
                    for (category, descriptors) in group_visible_descriptors(&query) {
                        ui.heading(category.label());
                        for x in descriptors {
                            let blocked = d.action_editor.draft.is_some();
                            let action = (x.make_default)();
                            let context_error = if matches!(x.editor, EditorKind::DirectInsert) {
                                d.selected_macro().and_then(|m| {
                                    validate_direct_context(m, insertion_position(d), &action).err()
                                })
                            } else {
                                None
                            };
                            let response = ui
                                .add_enabled(
                                    !blocked && context_error.is_none(),
                                    egui::Button::new(x.name),
                                )
                                .on_hover_text(x.description);
                            let response = if blocked {
                                response.on_disabled_hover_text(
                                    "Apply or cancel the current action first.",
                                )
                            } else if let Some(reason) = context_error {
                                response.on_disabled_hover_text(reason)
                            } else {
                                response
                            };
                            if response.clicked() {
                                select_descriptor(d, &x);
                            }
                        }
                    }
                });
            ui.separator();
            close_clicked = ui.button("Close").clicked();
        });
    if !open || close_clicked {
        close(d);
    }
}
