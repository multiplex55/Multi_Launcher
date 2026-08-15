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
    Structural,
    DirectInsert,
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
        confidence: 0.8,
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
    ($c:ident,$n:literal,$desc:literal,$keys:expr,$a:expr) => {
        ActionDescriptor {
            category: ActionCategory::$c,
            availability: ActionAvailability::Ready,
            name: $n,
            description: $desc,
            keywords: $keys,
            make_default: || $a,
            editor: editor_for_action(&$a),
            runtime: RuntimeAvailability::Supported,
        }
    };
    (hidden,$c:ident,$n:literal,$desc:literal,$keys:expr,$a:expr) => {
        ActionDescriptor {
            category: ActionCategory::$c,
            availability: ActionAvailability::Hidden,
            name: $n,
            description: $desc,
            keywords: $keys,
            make_default: || $a,
            editor: editor_for_action(&$a),
            runtime: RuntimeAvailability::Unavailable,
        }
    };
    (direct,$c:ident,$n:literal,$desc:literal,$keys:expr,$a:expr) => {
        ActionDescriptor {
            category: ActionCategory::$c,
            // Terminators and context-sensitive control markers are generated
            // by structural insertion, never offered as unsafe standalone rows.
            availability: ActionAvailability::Hidden,
            name: $n,
            description: $desc,
            keywords: $keys,
            make_default: || $a,
            editor: EditorKind::DirectInsert,
            runtime: RuntimeAvailability::Supported,
        }
    };
}
pub fn descriptors() -> Vec<ActionDescriptor> {
    let mut entries = vec![
        d!(
            KeyboardText,
            "Key Press",
            "Press and release a keyboard key",
            &["keyboard", "send"],
            MkAction::KeyPress(MkKey::Enter)
        ),
        d!(
            KeyboardText,
            "Key Down",
            "Hold a keyboard key",
            &["keyboard"],
            MkAction::KeyDown(MkKey::Enter)
        ),
        d!(
            KeyboardText,
            "Key Up",
            "Release a keyboard key",
            &["keyboard"],
            MkAction::KeyUp(MkKey::Enter)
        ),
        d!(
            KeyboardText,
            "Hotkey",
            "Send a key combination",
            &["keyboard", "send"],
            MkAction::Hotkey(vec![MkKey::Control, MkKey::Character("C".into())])
        ),
        d!(
            KeyboardText,
            "Text",
            "Type or paste text",
            &["type", "send", "keyboard"],
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
            MkAction::MouseDown(MkMouseButton::Left)
        ),
        d!(
            Mouse,
            "Mouse Up",
            "Release a mouse button",
            &["mouse"],
            MkAction::MouseUp(MkMouseButton::Left)
        ),
        d!(
            Mouse,
            "Mouse Scroll",
            "Scroll the mouse wheel",
            &["mouse"],
            MkAction::MouseScroll { i32_delta: -120 }
        ),
        d!(
            Timing,
            "Delay",
            "Wait for a duration",
            &["wait"],
            MkAction::Delay { milliseconds: 1000 }
        ),
        d!(
            Timing,
            "Wait Until",
            "Wait for a condition",
            &["wait", "condition"],
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
            MkAction::WindowActivate(wp())
        ),
        d!(
            Windows,
            "Close Window",
            "Close a matching window",
            &["window"],
            MkAction::WindowClose(matcher())
        ),
        d!(
            Windows,
            "Wait for Window",
            "Wait for a matching window",
            &["window", "wait"],
            MkAction::WindowWait(wp())
        ),
        d!(
            ProgramsLauncher,
            "Run Program",
            "Start a program",
            &["run", "launch"],
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
            MkAction::UnsetVariable {
                name: "value".into()
            }
        ),
        d!(
            Logic,
            "If",
            "Condition block",
            &["condition"],
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
            MkAction::RepeatStart { count: 2 }
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
            MkAction::ImageFind(ip())
        ),
        d!(
            Visual,
            "Click Image",
            "Find and click an image",
            &["image", "click"],
            MkAction::ImageClick(ip())
        ),
        d!(
            Visual,
            "Check Pixel",
            "Check a pixel color",
            &["pixel"],
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
            MkAction::UiInvoke(up())
        ),
        d!(
            hidden,
            UiAutomation,
            "Set UI Value",
            "Set an element value",
            &["uia"],
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
            MkAction::UiToggle(up())
        ),
        d!(
            hidden,
            UiAutomation,
            "Select UI Element",
            "Select an element",
            &["uia"],
            MkAction::UiSelect(up())
        ),
        d!(
            hidden,
            UiAutomation,
            "Focus UI Element",
            "Focus an element",
            &["uia"],
            MkAction::UiFocus(up())
        ),
        d!(
            hidden,
            UiAutomation,
            "Wait for UI Element",
            "Wait for an element",
            &["uia", "wait"],
            MkAction::UiWait(up())
        ),
    ];
    for descriptor in &mut entries {
        let action = (descriptor.make_default)();
        if matches!(
            action,
            MkAction::WaitUntil { .. }
                | MkAction::SetVariable { .. }
                | MkAction::UnsetVariable { .. }
        ) {
            descriptor.availability = ActionAvailability::Hidden;
        }
    }
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
        MkAction::WindowActivate(_) | MkAction::WindowClose(_) | MkAction::WindowWait(_) => {
            EditorKind::Window
        }
        MkAction::If(_) | MkAction::RepeatStart { .. } | MkAction::WhileStart { .. } => {
            EditorKind::Structural
        }
        MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatEnd
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue => EditorKind::DirectInsert,
        MkAction::ImageFind(_) | MkAction::ImageClick(_) => EditorKind::Image,
        MkAction::PixelCheck { .. } => EditorKind::Pixel,
        MkAction::WaitUntil { .. }
        | MkAction::SetVariable { .. }
        | MkAction::UnsetVariable { .. }
        | MkAction::UiInvoke(_)
        | MkAction::UiSetValue { .. }
        | MkAction::UiReadValue { .. }
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => EditorKind::Structural,
    }
}

/// True only when this exact action/editor pairing has an implemented route.
pub fn editor_route_recognizes(action: &MkAction, editor: EditorKind) -> bool {
    editor_for_action(action) == editor
}

/// A pure description used by rendering-contract tests (and useful to
/// accessibility tooling). Zero means the strategy is deliberately insertion-only.
pub fn editable_field_count(editor: EditorKind) -> usize {
    match editor {
        EditorKind::DirectInsert | EditorKind::Structural => 0,
        EditorKind::Keyboard
        | EditorKind::Text
        | EditorKind::Timing
        | EditorKind::MouseButton
        | EditorKind::MouseScroll
        | EditorKind::Process
        | EditorKind::Launcher => 1,
        EditorKind::MouseMove | EditorKind::MouseClick | EditorKind::Image | EditorKind::Pixel => 2,
        EditorKind::MouseDrag | EditorKind::Window => 3,
    }
}
/// Descriptors currently offered by the macro-authoring UI.
pub fn visible_descriptors() -> impl Iterator<Item = ActionDescriptor> {
    descriptors()
        .into_iter()
        .filter(|descriptor| descriptor.availability == ActionAvailability::Ready)
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
        MkAction::PixelCheck { .. } => "Check Pixel",
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
            "Asset {} · {:.0}% confidence · screen · {} ms timeout",
            p.asset_id,
            p.confidence * 100.0,
            p.wait.timeout_ms
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
    let ids = d.selection.ids.clone();
    let Some(m) = d.selected_macro_mut() else {
        return;
    };
    let selected: Vec<usize> = m
        .steps
        .iter()
        .enumerate()
        .filter_map(|(i, s)| ids.contains(&s.id).then_some(i))
        .collect();
    let pos = selected.last().map_or(m.steps.len(), |i| i + 1);
    let terminator = match &action {
        MkAction::If(_) => Some(MkAction::EndIf),
        MkAction::RepeatStart { .. } => Some(MkAction::RepeatEnd),
        MkAction::WhileStart { .. } => Some(MkAction::WhileEnd),
        _ => None,
    };
    let inserted_indices = if let Some(terminator) = terminator {
        if let (Some(first), Some(last)) = (selected.first(), selected.last()) {
            let first = *first;
            let last = *last;
            let terminator_index = last + 2;
            m.steps.insert(first, step(action));
            m.steps.insert(terminator_index, step(terminator));
            vec![first, terminator_index]
        } else {
            m.steps.insert(pos, step(action));
            m.steps.insert(pos + 1, step(terminator));
            vec![pos, pos + 1]
        }
    } else {
        m.steps.insert(pos, step(action));
        vec![pos]
    };
    repair_ids(&mut d.draft);
    let chosen = inserted_indices
        .iter()
        .map(|&index| d.selected_macro().unwrap().steps[index].id)
        .collect();
    d.selection.ids = chosen;
    d.mark_dirty()
}
/// Select a catalog entry without ever replacing an in-progress transaction.
pub fn select_descriptor(d: &mut MkMacroDialog, descriptor: &ActionDescriptor) -> bool {
    if d.action_editor.draft.is_some() {
        return false;
    }
    let action = (descriptor.make_default)();
    assert!(
        editor_route_recognizes(&action, descriptor.editor),
        "catalog editor/action mismatch for {}",
        descriptor.name
    );
    match descriptor.editor {
        EditorKind::DirectInsert | EditorKind::Structural => insert_action(d, action),
        kind => d.action_editor.begin_new_with_editor(action, kind),
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
                        let response = ui
                            .add_enabled(!blocked, egui::Button::new(x.name))
                            .on_hover_text(x.description);
                        let response = if blocked {
                            response
                                .on_disabled_hover_text("Apply or cancel the current action first.")
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
