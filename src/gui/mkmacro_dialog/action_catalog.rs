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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionAvailability {
    Ready,
    Hidden,
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
        title: Some(String::new()),
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
        asset_id: 0,
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
        }
    };
}
pub fn descriptors() -> Vec<ActionDescriptor> {
    vec![
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
            MkAction::MouseMove(point())
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
                program: String::new(),
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
                command: String::new(),
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
            Logic,
            "Else",
            "Alternate condition branch",
            &["condition"],
            MkAction::Else
        ),
        d!(
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
            Logic,
            "End While",
            "End while loop",
            &["loop"],
            MkAction::WhileEnd
        ),
        d!(Logic, "Break", "Exit a loop", &["loop"], MkAction::Break),
        d!(
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
    ]
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
        MkAction::MouseClick(p) => format!("{} click ×{}", mouse(&p.button), p.clicks),
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
            "Image asset {} ({:.0}% confidence)",
            p.asset_id,
            p.confidence * 100.0
        ),
        MkAction::PixelCheck {
            color, tolerance, ..
        } => format!("Color {color}, tolerance {tolerance}"),
        MkAction::UiSetValue { value, .. } => {
            format!("Unavailable UI Automation action (set value to {value})")
        }
        MkAction::UiReadValue { variable, .. } => {
            format!("Unavailable UI Automation action (read into {variable})")
        }
        MkAction::MouseMove(_) => "Target coordinates".into(),
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
        | MkAction::Continue => String::new(),
        MkAction::UiInvoke(_)
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => "Unavailable UI Automation action (saved target preserved)".into(),
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
pub(super) fn show_modal(ctx: &egui::Context, d: &mut MkMacroDialog) {
    if !d.action_catalog_visible {
        return;
    }
    let mut open = true;
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
                        if ui.button(x.name).on_hover_text(x.description).clicked() {
                            let action = (x.make_default)();
                            if matches!(
                                action,
                                MkAction::MouseClick(_)
                                    | MkAction::MouseMove(_)
                                    | MkAction::KeyPress(_)
                                    | MkAction::KeyDown(_)
                                    | MkAction::KeyUp(_)
                                    | MkAction::Hotkey(_)
                                    | MkAction::Text(_)
                                    | MkAction::Delay { .. }
                                    | MkAction::Process(_)
                                    | MkAction::WindowActivate(_)
                                    | MkAction::WindowWait(_)
                                    | MkAction::LauncherCommand { .. }
                            ) {
                                d.action_editor.begin_new(action);
                            } else {
                                insert_action(d, action);
                            }
                            d.action_catalog_visible = false;
                        }
                    }
                });
        });
    d.action_catalog_visible &= open
}
