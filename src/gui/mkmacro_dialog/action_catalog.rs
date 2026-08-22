use super::MkMacroDialog;
use crate::mkmacro::variables::{MkPoint, MkValue};
use crate::mkmacro::*;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionCategory {
    KeyboardText,
    Mouse,
    Timing,
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
    Pixel,
    Condition,
    Repeat,
    Variable,
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
fn ip() -> MkImagePayload {
    MkImagePayload {
        asset_id: 1,
        wait: wait(),
        region: SearchRegion::Desktop,
        tolerance: 0,
        alpha: AlphaPolicy::Compare,
        return_point: ReturnPoint::Center,
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
            MkAction::MouseScroll { i32_delta: -120 }
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
            hidden,
            Visual,
            "Find Image",
            "Find an image",
            &["image"],
            Image,
            "Image search requires a production visual-search backend before it can be inserted",
            MkAction::ImageFind(ip())
        ),
        d!(
            hidden,
            Visual,
            "Click Image",
            "Find and click an image",
            &["image", "click"],
            Image,
            "Image search requires a production visual-search backend before it can be inserted",
            MkAction::ImageClick(ip())
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
        MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatEnd
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue => EditorKind::DirectInsert,
        MkAction::ImageFind(_) | MkAction::ImageClick(_) => EditorKind::Image,
        MkAction::PixelCheck { .. } => EditorKind::Pixel,
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
    const PLACEHOLDERS: [&str; 5] = [
        "existing specialized editor",
        "legacy action",
        "not implemented",
        "unavailable",
        "placeholder",
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
        EditorKind::MouseMove | EditorKind::MouseClick | EditorKind::Image | EditorKind::Pixel => {
            Some(EditorContract::Configurable { field_count: 2 })
        }
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
        MkAction::ImageClick(_) => "Click Image",
        MkAction::PixelCheck { .. } => "Check Pixel Color",
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
    match a {
        MkAction::KeyDown(k) | MkAction::KeyUp(k) | MkAction::KeyPress(k) => {
            super::key_capture::key_name(k)
        }
        MkAction::Hotkey(k) => k
            .iter()
            .map(super::key_capture::key_name)
            .collect::<Vec<_>>()
            .join(" + "),
        MkAction::Text(p) => format!("{} characters", p.text.chars().count()),
        MkAction::MouseClick(p) => format!(
            "{} ×{} @ {}",
            mouse(&p.button),
            p.clicks,
            format_coordinate_target(&p.target)
        ),
        MkAction::MouseDown(b) | MkAction::MouseUp(b) => mouse(b).into(),
        MkAction::MouseScroll { i32_delta } => format!("{i32_delta} wheel units"),
        MkAction::Delay { milliseconds } => format!("{milliseconds} ms"),
        MkAction::Process(p) => format!("{} {}", p.program, p.arguments.join(" ")),
        MkAction::LauncherCommand { command, args } => {
            format!("{} {}", command, args.as_deref().unwrap_or(""))
        }
        MkAction::SetVariable { name, .. } => format!("Set {name}"),
        MkAction::UnsetVariable { name } => format!("Unset {name}"),
        MkAction::RepeatStart { count } => format!("{count} times"),
        MkAction::ImageFind(p) | MkAction::ImageClick(p) => format!(
            "Reference image · {:?} · tolerance {} · {} ms timeout",
            p.region, p.tolerance, p.wait.timeout_ms
        ),
        MkAction::PixelCheck {
            target,
            color,
            tolerance,
        } => format!(
            "{color} ±{tolerance} @ {}",
            format_coordinate_target(target)
        ),
        MkAction::UiSetValue { value, .. } => {
            format!("Unavailable UI Automation action (set value to {value})")
        }
        MkAction::UiReadValue { variable, .. } => {
            format!("Unavailable UI Automation action (read into {variable})")
        }
        MkAction::MouseMove(p) => format!(
            "{} · {}",
            format_coordinate_target(&p.target),
            if p.duration_ms == 0 {
                "Instant".into()
            } else {
                format!("{} ms", p.duration_ms)
            }
        ),
        MkAction::MouseDrag(p) => format!(
            "{} → {} · {} · {} ms",
            format_coordinate_target(&p.from),
            format_coordinate_target(&p.to),
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
        MkAction::WaitUntil { wait, .. } => format!("Timeout {} ms", wait.timeout_ms),
        MkAction::If(_) | MkAction::WhileStart { .. } => "Condition".into(),
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
pub fn format_coordinate_target(target: &MkCoordinateTarget) -> String {
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
            format!("Image asset {asset_id} offset ({}, {})", offset.x, offset.y)
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
                    let mut last = None;
                    let query = d.action_search.clone();
                    for x in visible_descriptors().filter(|x| matches(x, &query)) {
                        if last != Some(x.category) {
                            ui.heading(x.category.label());
                            last = Some(x.category)
                        }
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
                            response
                                .on_disabled_hover_text("Apply or cancel the current action first.")
                        } else if let Some(reason) = context_error {
                            response.on_disabled_hover_text(reason)
                        } else {
                            response
                        };
                        if response.clicked() {
                            select_descriptor(d, &x);
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
