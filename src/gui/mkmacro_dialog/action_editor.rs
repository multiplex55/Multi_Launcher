//! Transactional, typed action editing.
//!
//! The editor owns a complete `MkStep` clone.  No document field is borrowed by
//! the modal, which makes closing/cancelling it a genuinely lossless operation.
use super::{
    MkMacroDialog,
    key_capture::{CapturedChord, captured_chord, key_name},
};
use crate::mkmacro::variables::{MkPoint, MkValue};
use crate::mkmacro::*;
use eframe::egui;

#[derive(Default)]
pub struct ActionEditorState {
    pub draft: Option<MkStep>,
    /// `None` means insert a new row; otherwise replace this stable step id.
    pub editing_id: Option<u64>,
    /// Captured when the editor opens, so applying cannot accidentally use a
    /// selection which changed underneath the modal.
    pub insertion: Option<InsertionIntent>,
    pub capture_keys: bool,
    pub capture_message: Option<String>,
    pub editor: Option<super::action_catalog::EditorKind>,
    position_capture: Option<PositionCaptureState>,
    pub(crate) draft_generation: u64,
    picker: NativePositionPicker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertionIntent {
    Plain { after_step_id: Option<u64> },
    Wrap { step_ids: Vec<u64> },
    EditExisting { step_id: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PositionCaptureSlot {
    MoveTarget,
    ClickTarget,
    DragFrom,
    DragTo,
    PixelPosition,
    PixelColor,
}
#[derive(Clone, Debug)]
struct PositionCaptureState {
    target_step_id: Option<u64>,
    draft_generation: u64,
    slot: PositionCaptureSlot,
    awaiting_release: bool,
    last_screen_position: Option<MkPoint>,
    #[cfg(windows)]
    foreground_window: isize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionCaptureEvent {
    PointerMoved(MkPoint),
    LeftPressed,
    LeftReleased,
    Enter,
    Escape,
}
pub trait PositionPicker {
    /// Polls once and returns immediately. Confirming clicks are deliberately
    /// allowed through to the underlying application (the launcher never hooks
    /// or suppresses input).
    fn poll_event(&mut self) -> Result<Option<PositionCaptureEvent>, String>;
}

#[derive(Default)]
struct NativePositionPicker {
    #[cfg(windows)]
    last_position: Option<MkPoint>,
    #[cfg(windows)]
    left_down: bool,
    #[cfg(windows)]
    enter_down: bool,
    #[cfg(windows)]
    escape_down: bool,
}

impl ActionEditorState {
    pub fn apply_window_matcher(
        &mut self,
        request: &super::window_picker::MatcherEditRequest,
        matcher: MkWindowMatcher,
        current_macro_id: Option<u64>,
    ) -> bool {
        let super::window_picker::MatcherDestination::Action {
            macro_id,
            draft_generation,
            path,
        } = &request.destination;
        if Some(*macro_id) != current_macro_id || *draft_generation != self.draft_generation {
            return false;
        }
        let Some(step) = self.draft.as_mut() else {
            return false;
        };
        let Some(target) = matcher_at_path(&mut step.action, path) else {
            return false;
        };
        if *target != matcher {
            *target = matcher;
        }
        true
    }
    pub fn begin_new(&mut self, action: MkAction) {
        let editor = super::action_catalog::editor_for_action(&action);
        self.begin_new_with_editor(action, editor);
    }
    pub fn begin_new_with_editor(
        &mut self,
        action: MkAction,
        editor: super::action_catalog::EditorKind,
    ) {
        if self.draft.is_some() {
            return;
        }
        self.stop_position_capture();
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.editing_id = None;
        // The dialog supplies the precise insertion intent immediately after
        // this call. This fallback keeps programmatic callers insertion-safe.
        self.insertion = Some(InsertionIntent::Plain {
            after_step_id: None,
        });
        self.editor = Some(editor);
        self.draft = Some(MkStep {
            id: 0,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        });
    }
    pub fn begin_edit(&mut self, step: &MkStep) {
        self.stop_position_capture();
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.editing_id = Some(step.id);
        self.insertion = Some(InsertionIntent::EditExisting { step_id: step.id });
        self.draft = Some(step.clone());
        self.editor = Some(super::action_catalog::editor_for_action(&step.action));
    }
    pub fn cancel(&mut self) {
        self.stop_position_capture();
        self.draft = None;
        self.editing_id = None;
        self.insertion = None;
        self.capture_keys = false;
        self.editor = None;
    }
    pub fn apply(&mut self, dialog: &mut MkMacroDialog) -> Option<u64> {
        self.stop_position_capture();
        let mut step = self.draft.take()?;
        let edit = self.editing_id.take();
        let intent = self.insertion.take().unwrap_or_else(|| match edit {
            Some(step_id) => InsertionIntent::EditExisting { step_id },
            None => InsertionIntent::Plain {
                after_step_id: None,
            },
        });
        self.editor = None;
        // New block openers are inserted together with their mandatory closing
        // marker, while the configured step settings remain on the opener.
        if !matches!(intent, InsertionIntent::EditExisting { .. })
            && matches!(
                &step.action,
                MkAction::If(_) | MkAction::RepeatStart { .. } | MkAction::WhileStart { .. }
            )
        {
            return match super::action_catalog::apply_structural(dialog, step, intent) {
                Ok(id) => Some(id),
                Err(error) => {
                    dialog.command_error = Some(error);
                    None
                }
            };
        }
        let m = dialog.selected_macro_mut()?;
        let index = if let InsertionIntent::EditExisting { step_id: id } = intent {
            let i = m.steps.iter().position(|s| s.id == id)?;
            step.id = id;
            m.steps[i] = step;
            i
        } else {
            step.id = 0;
            let after_step_id = match intent {
                InsertionIntent::Plain { after_step_id } => after_step_id,
                InsertionIntent::Wrap { .. } | InsertionIntent::EditExisting { .. } => None,
            };
            let i = after_step_id
                .and_then(|id| m.steps.iter().position(|s| s.id == id))
                .map_or(m.steps.len(), |i| i + 1);
            m.steps.insert(i, step);
            i
        };
        crate::mkmacro::repair_ids(&mut dialog.draft);
        let id = dialog.selected_macro()?.steps[index].id;
        dialog.selection.ids.clear();
        dialog.selection.ids.insert(id);
        dialog.mark_dirty();
        Some(id)
    }
    fn stop_position_capture(&mut self) {
        self.position_capture = None;
        self.capture_message = None;
    }

    fn start_position_capture(&mut self, slot: PositionCaptureSlot) -> Result<(), String> {
        #[cfg(not(windows))]
        {
            let _ = slot;
            return Err("Position capture is available only on Windows".into());
        }
        #[cfg(windows)]
        {
            use ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
            self.position_capture = Some(PositionCaptureState {
                target_step_id: self.editing_id,
                draft_generation: self.draft_generation,
                slot,
                awaiting_release: true,
                last_screen_position: None,
                foreground_window: unsafe { GetForegroundWindow() }.0 as isize,
            });
            self.capture_message = None;
            Ok(())
        }
    }

    fn target_mut(&mut self, slot: PositionCaptureSlot) -> Option<&mut MkCoordinateTarget> {
        let action = &mut self.draft.as_mut()?.action;
        match (slot, action) {
            (PositionCaptureSlot::MoveTarget, MkAction::MouseMove(p)) => Some(&mut p.target),
            (PositionCaptureSlot::ClickTarget, MkAction::MouseClick(p)) => Some(&mut p.target),
            (PositionCaptureSlot::DragFrom, MkAction::MouseDrag(p)) => Some(&mut p.from),
            (PositionCaptureSlot::DragTo, MkAction::MouseDrag(p)) => Some(&mut p.to),
            (PositionCaptureSlot::PixelPosition, MkAction::PixelCheck { target, .. }) => {
                Some(target)
            }
            _ => None,
        }
    }

    fn process_position_event(&mut self, event: PositionCaptureEvent) {
        let Some(mut capture) = self.position_capture.take() else {
            return;
        };
        if capture.target_step_id != self.editing_id
            || capture.draft_generation != self.draft_generation
        {
            self.capture_message =
                Some("Position capture cancelled because the edited action changed".into());
            return;
        }
        match event {
            PositionCaptureEvent::PointerMoved(p) => {
                capture.last_screen_position = Some(p);
                self.position_capture = Some(capture);
            }
            PositionCaptureEvent::LeftReleased if capture.awaiting_release => {
                capture.awaiting_release = false;
                self.position_capture = Some(capture);
            }
            PositionCaptureEvent::LeftPressed if !capture.awaiting_release => {
                self.confirm_position(capture)
            }
            PositionCaptureEvent::Enter => self.confirm_position(capture),
            PositionCaptureEvent::Escape => {
                self.capture_message = Some("Position capture cancelled".into());
            }
            _ => self.position_capture = Some(capture),
        }
    }

    fn confirm_position(&mut self, capture: PositionCaptureState) {
        self.confirm_position_with(capture, read_live_desktop_pixel);
    }

    fn confirm_position_with(
        &mut self,
        capture: PositionCaptureState,
        read_pixel: impl FnOnce(MkPoint) -> Result<[u8; 3], String>,
    ) {
        let Some(screen) = capture.last_screen_position else {
            self.capture_message = Some("Unable to read the current pointer position".into());
            return;
        };
        if capture.slot == PositionCaptureSlot::PixelColor {
            let result = read_pixel(screen).map(|rgb| {
                if let Some(MkStep {
                    action: MkAction::PixelCheck { color, .. },
                    ..
                }) = self.draft.as_mut()
                {
                    *color = crate::mkmacro::screen::format_rgb(rgb);
                }
            });
            self.capture_message = result.err();
            return;
        }
        let needs_matched = matches!(
            self.target_mut(capture.slot),
            Some(MkCoordinateTarget::WindowClient { .. })
        );
        let window = if needs_matched {
            picked_window_at(screen)
        } else {
            picked_foreground_window(&capture)
        };
        let result = match self.target_mut(capture.slot) {
            Some(
                target @ (MkCoordinateTarget::ActiveWindow { .. }
                | MkCoordinateTarget::WindowClient { .. }),
            ) => window.and_then(|w| apply_picked_position(target, screen, Some(&w))),
            Some(target) => apply_picked_position(target, screen, None),
            None => Err("The captured field no longer exists".into()),
        };
        self.capture_message = result.err();
    }
    /// Applies a platform-independent captured chord. Modifier-only captures are invalid.
    pub fn set_captured_keys(&mut self, mut keys: Vec<MkKey>) -> bool {
        let Some(step) = &mut self.draft else {
            return false;
        };
        keys.dedup();
        let Some(primary) = keys.iter().rposition(|k| !is_modifier(k)) else {
            return false;
        };
        let key = keys.remove(primary);
        step.action = match step.action {
            MkAction::KeyDown(_) => MkAction::KeyDown(key),
            MkAction::KeyUp(_) => MkAction::KeyUp(key),
            _ if keys.is_empty() => MkAction::KeyPress(key),
            _ => {
                keys.push(key);
                MkAction::Hotkey(keys)
            }
        };
        self.capture_keys = false;
        true
    }
}

#[cfg(windows)]
fn read_live_desktop_pixel(point: MkPoint) -> Result<[u8; 3], String> {
    let rgba = WindowsScreenCaptureBackend::system()
        .read_pixel(point)
        .map_err(|e| e.to_string())?;
    Ok([rgba[0], rgba[1], rgba[2]])
}
#[cfg(not(windows))]
fn read_live_desktop_pixel(_: MkPoint) -> Result<[u8; 3], String> {
    Err("Desktop color picking is available only on Windows".into())
}

fn is_modifier(k: &MkKey) -> bool {
    matches!(
        k,
        MkKey::Control
            | MkKey::LeftControl
            | MkKey::RightControl
            | MkKey::Alt
            | MkKey::LeftAlt
            | MkKey::RightAlt
            | MkKey::Shift
            | MkKey::LeftShift
            | MkKey::RightShift
            | MkKey::Meta
            | MkKey::LeftMeta
            | MkKey::RightMeta
    )
}

#[cfg(windows)]
impl PositionPicker for NativePositionPicker {
    fn poll_event(&mut self) -> Result<Option<PositionCaptureEvent>, String> {
        use ::windows::Win32::{
            Foundation::POINT,
            UI::{
                Input::KeyboardAndMouse::{
                    GetAsyncKeyState, VIRTUAL_KEY, VK_ESCAPE, VK_LBUTTON, VK_RETURN,
                },
                WindowsAndMessaging::GetCursorPos,
            },
        };
        let mut raw = POINT::default();
        unsafe { GetCursorPos(&mut raw) }.map_err(|e| format!("GetCursorPos failed: {e}"))?;
        let point = MkPoint { x: raw.x, y: raw.y };
        if self.last_position != Some(point) {
            self.last_position = Some(point);
            return Ok(Some(PositionCaptureEvent::PointerMoved(point)));
        }
        let down = |key: VIRTUAL_KEY| unsafe { GetAsyncKeyState(key.0 as i32) } < 0;
        let left = down(VK_LBUTTON);
        let enter = down(VK_RETURN);
        let escape = down(VK_ESCAPE);
        let event = if escape && !self.escape_down {
            Some(PositionCaptureEvent::Escape)
        } else if enter && !self.enter_down {
            Some(PositionCaptureEvent::Enter)
        } else if left != self.left_down {
            Some(if left {
                PositionCaptureEvent::LeftPressed
            } else {
                PositionCaptureEvent::LeftReleased
            })
        } else {
            None
        };
        self.left_down = left;
        self.enter_down = enter;
        self.escape_down = escape;
        Ok(event)
    }
}
#[cfg(not(windows))]
impl PositionPicker for NativePositionPicker {
    fn poll_event(&mut self) -> Result<Option<PositionCaptureEvent>, String> {
        Err("Position capture is available only on Windows".into())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PickedWindow {
    pub matcher: MkWindowMatcher,
    pub client_origin: MkPoint,
}

#[cfg(windows)]
fn picked_foreground_window(c: &PositionCaptureState) -> Result<PickedWindow, String> {
    use ::windows::Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::GetForegroundWindow,
    };
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() || hwnd != HWND(c.foreground_window as *mut core::ffi::c_void) {
        return Err("The active window changed or disappeared during capture".into());
    }
    let mut origin = POINT::default();
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return Err(format!(
            "Could not determine the active window client origin: {}",
            ::windows::core::Error::from_win32()
        ));
    }
    Ok(PickedWindow {
        matcher: MkWindowMatcher {
            title: None,
            title_regex: None,
            process: None,
            class: None,
        },
        client_origin: MkPoint {
            x: origin.x,
            y: origin.y,
        },
    })
}
#[cfg(not(windows))]
fn picked_foreground_window(_: &PositionCaptureState) -> Result<PickedWindow, String> {
    Err("Active-window capture is available only on Windows".into())
}
#[cfg(windows)]
fn picked_window_at(screen: MkPoint) -> Result<PickedWindow, String> {
    use ::windows::Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, IsWindow, WindowFromPoint},
    };
    let child = unsafe {
        WindowFromPoint(POINT {
            x: screen.x,
            y: screen.y,
        })
    };
    let root = unsafe { GetAncestor(child, GA_ROOT) };
    if child.0.is_null() || root.0.is_null() || !unsafe { IsWindow(root) }.as_bool() {
        return Err("The pointed window disappeared; move the pointer and retry".into());
    }
    let handle = root.0 as usize;
    let candidate = crate::multi_manager::win::enumerate_top_level_windows()
        .map_err(|e| format!("Could not read the pointed window identity: {e}"))?
        .into_iter()
        .find(|w| w.hwnd == handle)
        .ok_or("The pointed window disappeared before its identity could be read")?;
    let mut origin = POINT::default();
    if !unsafe { ClientToScreen(HWND(root.0), &mut origin) }.as_bool() {
        return Err("Could not determine the pointed window client origin".into());
    }
    Ok(PickedWindow {
        matcher: MkWindowMatcher {
            title: Some(candidate.title),
            title_regex: None,
            process: Some(candidate.executable),
            class: None,
        },
        client_origin: MkPoint {
            x: origin.x,
            y: origin.y,
        },
    })
}
#[cfg(not(windows))]
fn picked_window_at(_: MkPoint) -> Result<PickedWindow, String> {
    Err("Matched-window capture is available only on Windows".into())
}
pub trait WindowPicker {
    fn pick_window(&mut self) -> Result<Option<PickedWindow>, String>;
}

pub fn apply_picked_position(
    target: &mut MkCoordinateTarget,
    screen: MkPoint,
    window: Option<&PickedWindow>,
) -> Result<(), String> {
    match target {
        MkCoordinateTarget::Screen { point } => *point = screen,
        MkCoordinateTarget::ActiveWindow { point } => {
            let w = window.ok_or("No matching active window is available")?;
            *point = translated_client_point(screen, w.client_origin)?;
        }
        MkCoordinateTarget::WindowClient { matcher, point } => {
            let w = window.ok_or("No pointed window is available")?;
            let candidate = crate::mkmacro::windows::WindowCandidate {
                handle: 0,
                title: w.matcher.title.clone().unwrap_or_default(),
                executable: w.matcher.process.clone().unwrap_or_default(),
                process_path: String::new(),
                class_name: w.matcher.class.clone().unwrap_or_default(),
            };
            if !crate::mkmacro::windows::candidate_matches(matcher, &candidate)
                .map_err(|e| e.to_string())?
            {
                return Err("The pointed window does not match this target. Use Choose Window… to deliberately replace the matcher, then pick the position again".into());
            }
            *point = translated_client_point(screen, w.client_origin)?;
        }
        _ => return Err("This target does not store a fixed position".into()),
    }
    Ok(())
}
fn translated_client_point(screen: MkPoint, origin: MkPoint) -> Result<MkPoint, String> {
    Ok(MkPoint {
        x: screen
            .x
            .checked_sub(origin.x)
            .ok_or("Client-relative X coordinate overflow")?,
        y: screen
            .y
            .checked_sub(origin.y)
            .ok_or("Client-relative Y coordinate overflow")?,
    })
}
pub fn apply_picked_window(payload: &mut MkWindowPayload, picked: &PickedWindow) {
    payload.matcher = picked.matcher.clone();
}

fn optional_field(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let v = value.get_or_insert_with(String::new);
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(v);
    });
    if value.as_ref().is_some_and(|x| x.is_empty()) {
        *value = None;
    }
}
pub(super) fn matcher_ui(ui: &mut egui::Ui, m: &mut MkWindowMatcher) -> bool {
    optional_field(ui, "Executable", &mut m.process);
    optional_field(ui, "Title contains", &mut m.title);
    optional_field(ui, "Title regex", &mut m.title_regex);
    optional_field(ui, "Class", &mut m.class);
    ui.button("Choose Window…").clicked()
}
#[derive(Default)]
pub(super) struct TargetUiOutcome {
    pick_position: bool,
    pick_matcher: bool,
}
pub(super) fn target_ui(ui: &mut egui::Ui, target: &mut MkCoordinateTarget) -> TargetUiOutcome {
    let kind = match target {
        MkCoordinateTarget::Screen { .. } => 0,
        MkCoordinateTarget::ActiveWindow { .. } => 1,
        MkCoordinateTarget::WindowClient { .. } => 2,
        MkCoordinateTarget::Variable { .. } => 3,
        MkCoordinateTarget::Image { .. } => 4,
    };
    let mut next = kind;
    egui::ComboBox::from_label("Target")
        .selected_text(
            [
                "Screen",
                "Active Window",
                "Matched Window",
                "Variable",
                "Image Result",
            ][kind],
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut next, 0, "Screen");
            ui.selectable_value(&mut next, 1, "Active Window");
            ui.selectable_value(&mut next, 2, "Matched Window");
            ui.selectable_value(&mut next, 3, "Variable");
            ui.selectable_value(&mut next, 4, "Image Result");
        });
    if next != kind {
        *target = match next {
            0 => MkCoordinateTarget::Screen {
                point: MkPoint { x: 0, y: 0 },
            },
            1 => MkCoordinateTarget::ActiveWindow {
                point: MkPoint { x: 0, y: 0 },
            },
            2 => MkCoordinateTarget::WindowClient {
                matcher: MkWindowMatcher::default(),
                point: MkPoint { x: 0, y: 0 },
            },
            3 => MkCoordinateTarget::Variable {
                name: String::new(),
            },
            _ => MkCoordinateTarget::Image {
                asset_id: 0,
                offset: MkPoint { x: 0, y: 0 },
            },
        };
    }
    match target {
        MkCoordinateTarget::Screen { point } | MkCoordinateTarget::ActiveWindow { point } => {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut point.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut point.y));
            });
            #[cfg(windows)]
            let picked = ui.button("Pick Position").clicked();
            #[cfg(not(windows))]
            let picked = {
                ui.add_enabled(false, egui::Button::new("Pick Position (Windows only)"));
                false
            };
            return TargetUiOutcome {
                pick_position: picked,
                pick_matcher: false,
            };
        }
        MkCoordinateTarget::WindowClient { matcher, point } => {
            ui.heading("Window");
            let choose = matcher_ui(ui, matcher);
            ui.heading("Position");
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut point.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut point.y));
            });
            #[cfg(windows)]
            let position = ui.button("Pick Position").clicked();
            #[cfg(not(windows))]
            let position = {
                ui.add_enabled(false, egui::Button::new("Pick Position (Windows only)"));
                false
            };
            // The matcher picker is routed by the action-level caller; position capture remains
            // the boolean return for backward-compatible shared condition editing.
            return TargetUiOutcome {
                pick_position: position,
                pick_matcher: choose,
            };
        }
        MkCoordinateTarget::Variable { name } => {
            ui.horizontal(|ui| {
                ui.label("Variable");
                ui.text_edit_singleline(name);
            });
        }
        MkCoordinateTarget::Image { asset_id, offset } => {
            ui.horizontal(|ui| {
                ui.label("Image asset ID");
                ui.add(egui::DragValue::new(asset_id));
                ui.label("Offset X/Y");
                ui.add(egui::DragValue::new(&mut offset.x));
                ui.add(egui::DragValue::new(&mut offset.y));
            });
        }
    }
    TargetUiOutcome::default()
}

/// A shared typed editor. Switching type constructs a fresh value rather than
/// interpreting text belonging to the previous type.
pub(super) fn value_ui(ui: &mut egui::Ui, value: &mut MkValue) {
    let kind = match value {
        MkValue::String(_) => 0,
        MkValue::Number(_) => 1,
        MkValue::Boolean(_) => 2,
        MkValue::Point(_) => 3,
        MkValue::Null => 4,
    };
    let mut next = kind;
    egui::ComboBox::from_label("Type")
        .selected_text(["String", "Number", "Boolean", "Point", "Null"][kind])
        .show_ui(ui, |ui| {
            for (index, label) in ["String", "Number", "Boolean", "Point", "Null"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut next, index, *label);
            }
        });
    if next != kind {
        *value = match next {
            0 => MkValue::String(String::new()),
            1 => MkValue::Number(0.0),
            2 => MkValue::Boolean(false),
            3 => MkValue::Point(MkPoint { x: 0, y: 0 }),
            _ => MkValue::Null,
        };
    }
    match value {
        MkValue::String(text) => {
            ui.horizontal(|ui| {
                ui.label("Value");
                ui.text_edit_singleline(text);
            });
        }
        MkValue::Number(number) => {
            ui.horizontal(|ui| {
                ui.label("Value");
                ui.add(egui::DragValue::new(number));
            });
        }
        MkValue::Boolean(boolean) => {
            ui.checkbox(boolean, "Value");
        }
        MkValue::Point(point) => {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut point.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut point.y));
            });
        }
        MkValue::Null => {
            ui.small("Null has no value.");
        }
    }
}

fn action_ui(
    ui: &mut egui::Ui,
    step: &mut MkStep,
    capture: &mut bool,
    image_assets: &[u64],
) -> (
    Option<PositionCaptureSlot>,
    Option<super::window_picker::MatcherPath>,
    Option<super::launcher_action_picker::PickerPurpose>,
    Option<ImageAuthoringRequest>,
) {
    let mut pick = None;
    let mut window_pick = None;
    let mut launcher_pick = None;
    let mut image_request = None;
    match &mut step.action {
        MkAction::KeyPress(k) | MkAction::KeyDown(k) | MkAction::KeyUp(k) => {
            ui.label(format!("Captured: {}", key_name(k)));
            if ui.button("Capture next key or chord").clicked() {
                *capture = true;
            }
        }
        MkAction::Hotkey(keys) => {
            ui.label(format!(
                "Captured: {}",
                keys.iter().map(key_name).collect::<Vec<_>>().join(" + ")
            ));
            if ui.button("Capture next key or chord").clicked() {
                *capture = true;
            }
        }
        MkAction::Text(p) => {
            ui.add(
                egui::TextEdit::multiline(&mut p.text)
                    .desired_rows(10)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                ui.radio_value(&mut p.mode, MkTextMode::Type, "Type");
                ui.add_enabled(
                    false,
                    egui::RadioButton::new(p.mode == MkTextMode::Paste, "Paste (Coming later)"),
                );
            });
            if p.mode == MkTextMode::Paste {
                p.mode = MkTextMode::Type;
            }
        }
        MkAction::MouseMove(p) => {
            let response = target_ui(ui, &mut p.target);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::MoveTarget);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::MoveTarget,
                ));
            }
            let mut smooth = p.duration_ms != 0;
            ui.horizontal(|ui| {
                ui.label("Movement");
                ui.radio_value(&mut smooth, false, "Instant");
                ui.radio_value(&mut smooth, true, "Smooth");
            });
            if smooth {
                if p.duration_ms == 0 {
                    p.duration_ms = 250;
                }
                ui.add(
                    egui::DragValue::new(&mut p.duration_ms)
                        .clamp_range(1..=86_400_000)
                        .suffix(" ms"),
                );
            } else {
                p.duration_ms = 0;
            }
        }
        MkAction::MouseDrag(p) => {
            ui.label("Start");
            let response = target_ui(ui, &mut p.from);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::DragFrom);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::DragFrom,
                ));
            }
            ui.label("Destination");
            let response = target_ui(ui, &mut p.to);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::DragTo);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::DragTo,
                ));
            }
            egui::ComboBox::from_label("Button")
                .selected_text(format!("{:?}", p.button))
                .show_ui(ui, |ui| {
                    for b in [
                        MkMouseButton::Left,
                        MkMouseButton::Right,
                        MkMouseButton::Middle,
                        MkMouseButton::X1,
                        MkMouseButton::X2,
                    ] {
                        ui.selectable_value(&mut p.button, b.clone(), format!("{b:?}"));
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Duration (ms)");
                ui.add(egui::DragValue::new(&mut p.duration_ms));
            });
        }
        MkAction::MouseClick(p) => {
            let response = target_ui(ui, &mut p.target);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::ClickTarget);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::ClickTarget,
                ));
            }
            egui::ComboBox::from_label("Button")
                .selected_text(format!("{:?}", p.button))
                .show_ui(ui, |ui| {
                    for b in [
                        MkMouseButton::Left,
                        MkMouseButton::Right,
                        MkMouseButton::Middle,
                        MkMouseButton::X1,
                        MkMouseButton::X2,
                    ] {
                        ui.selectable_value(&mut p.button, b.clone(), format!("{b:?}"));
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Click count");
                ui.add(egui::DragValue::new(&mut p.clicks).clamp_range(1..=1_000_000));
            });
        }
        MkAction::MouseDown(button) | MkAction::MouseUp(button) => {
            egui::ComboBox::from_label("Button")
                .selected_text(format!("{button:?}"))
                .show_ui(ui, |ui| {
                    for b in [
                        MkMouseButton::Left,
                        MkMouseButton::Right,
                        MkMouseButton::Middle,
                        MkMouseButton::X1,
                        MkMouseButton::X2,
                    ] {
                        ui.selectable_value(button, b.clone(), format!("{b:?}"));
                    }
                });
        }
        MkAction::MouseScroll { i32_delta } => {
            const WHEEL: i32 = 120;
            if *i32_delta % WHEEL != 0 {
                ui.label(format!("Legacy raw wheel delta: {i32_delta}"));
                ui.small("Choose a direction to normalize this legacy value to whole notches.");
                ui.horizontal(|ui| {
                    if ui.button("Normalize Up").clicked() {
                        *i32_delta = WHEEL;
                    }
                    if ui.button("Normalize Down").clicked() {
                        *i32_delta = -WHEEL;
                    }
                });
            } else {
                let mut up = *i32_delta >= 0;
                let mut notches = (*i32_delta / WHEEL).unsigned_abs().max(1);
                let before = (up, notches);
                ui.horizontal(|ui| {
                    ui.label("Direction");
                    ui.radio_value(&mut up, true, "Vertical Up");
                    ui.radio_value(&mut up, false, "Vertical Down");
                });
                ui.add(
                    egui::DragValue::new(&mut notches)
                        .clamp_range(1..=(i32::MAX as u32 / WHEEL as u32))
                        .prefix("Notches "),
                );
                if before != (up, notches) || *i32_delta == 0 {
                    let magnitude = (notches as i32)
                        .checked_mul(WHEEL)
                        .unwrap_or(i32::MAX / WHEEL * WHEEL);
                    *i32_delta = if up { magnitude } else { -magnitude };
                }
            }
        }
        MkAction::Delay { milliseconds } => {
            ui.horizontal(|ui| {
                ui.label("Action duration (ms)");
                ui.add(egui::DragValue::new(milliseconds).clamp_range(0..=86_400_000));
            });
        }
        MkAction::Process(p) => {
            ui.horizontal(|ui| {
                ui.label("Program");
                ui.text_edit_singleline(&mut p.program);
                if ui.button("Browse…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Executable", &["exe", "bat", "cmd", "com"])
                        .pick_file()
                {
                    super::launcher_action_picker::apply_chosen_program_path(p, path);
                }
                if ui.button("Choose From Launcher…").clicked() {
                    launcher_pick = Some(super::launcher_action_picker::PickerPurpose::Process);
                }
            });
            let mut args = p.arguments.join(" ");
            ui.horizontal(|ui| {
                ui.label("Arguments");
                ui.text_edit_singleline(&mut args);
            });
            p.arguments = shlex::split(&args).unwrap_or_else(|| vec![args]);
            let wd = p.working_directory.get_or_insert_with(String::new);
            ui.horizontal(|ui| {
                ui.label("Working directory");
                ui.text_edit_singleline(wd);
                if ui.button("Browse…").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    *wd = path.to_string_lossy().into_owned();
                }
            });
            if wd.trim().is_empty() {
                p.working_directory = None;
            }
            ui.checkbox(&mut p.wait, "Wait for completion");
        }
        MkAction::WindowActivate(p) | MkAction::WindowWait(p) => {
            if matcher_ui(ui, &mut p.matcher) {
                window_pick = Some(super::window_picker::MatcherPath::Action);
            }
            if let Some(w) = &mut p.wait {
                ui.horizontal(|ui| {
                    ui.label("Timeout (ms)");
                    ui.add(egui::DragValue::new(&mut w.timeout_ms).clamp_range(0..=86_400_000));
                    ui.label("Poll (ms)");
                    ui.add(
                        egui::DragValue::new(&mut w.poll_interval_ms).clamp_range(1..=86_400_000),
                    );
                });
            }
        }
        MkAction::WindowClose(m) => {
            if matcher_ui(ui, m) {
                window_pick = Some(super::window_picker::MatcherPath::Action);
            }
        }
        MkAction::WindowMoveResize(p) => {
            if matcher_ui(ui, &mut p.matcher) {
                window_pick = Some(super::window_picker::MatcherPath::Action);
            }
            let mut moving = p.x.is_some() && p.y.is_some();
            if ui.checkbox(&mut moving, "Move").changed() {
                if moving {
                    p.x = Some(0);
                    p.y = Some(0);
                } else {
                    p.x = None;
                    p.y = None;
                }
            }
            if moving {
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(p.x.as_mut().unwrap()));
                    ui.label("Y");
                    ui.add(egui::DragValue::new(p.y.as_mut().unwrap()));
                });
            }
            let mut resizing = p.width.is_some() && p.height.is_some();
            if ui.checkbox(&mut resizing, "Resize").changed() {
                if resizing {
                    p.width = Some(1200);
                    p.height = Some(800);
                } else {
                    p.width = None;
                    p.height = None;
                }
            }
            if resizing {
                ui.horizontal(|ui| {
                    ui.label("Width");
                    ui.add(
                        egui::DragValue::new(p.width.as_mut().unwrap()).clamp_range(1..=u32::MAX),
                    );
                    ui.label("Height");
                    ui.add(
                        egui::DragValue::new(p.height.as_mut().unwrap()).clamp_range(1..=u32::MAX),
                    );
                });
            }
            if !moving && !resizing {
                ui.colored_label(ui.visuals().error_fg_color, "Enable Move or Resize");
            }
        }
        MkAction::WindowState { matcher, state } => {
            if matcher_ui(ui, matcher) {
                window_pick = Some(super::window_picker::MatcherPath::Action);
            }
            egui::ComboBox::from_label("Window state")
                .selected_text(format!("{state:?}"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(state, MkWindowState::Minimize, "Minimize");
                    ui.selectable_value(state, MkWindowState::Maximize, "Maximize");
                    ui.selectable_value(state, MkWindowState::Restore, "Restore");
                });
        }
        MkAction::SetVariable { name, value } => {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(name);
            });
            value_ui(ui, value);
        }
        MkAction::UnsetVariable { name } => {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(name);
            });
        }
        MkAction::PromptInput(p) => {
            ui.label("Dialog title");
            ui.text_edit_singleline(&mut p.title)
                .on_hover_text("Title of the independent input dialog");
            ui.label("Prompt text");
            ui.text_edit_multiline(&mut p.prompt);
            ui.label("Default value");
            ui.text_edit_singleline(&mut p.default_value);
            ui.label("Destination variable");
            ui.text_edit_singleline(&mut p.variable);
            ui.checkbox(&mut p.copy_to_clipboard, "Copy result to clipboard");
            if let Err(reason) = crate::mkmacro::variables::validate_variable_name(&p.variable) {
                ui.colored_label(ui.visuals().error_fg_color, reason);
            }
            ui.small("During playback, Cancel or closing the prompt stops the macro.");
        }
        MkAction::RepeatStart { count } => {
            ui.horizontal(|ui| {
                ui.label("Repeat count");
                ui.add(egui::DragValue::new(count).clamp_range(1..=1_000_000));
            });
        }
        MkAction::If(condition) | MkAction::WhileStart { condition } => {
            if let Some(path) =
                super::condition_editor::condition_ui_with_assets(ui, condition, image_assets)
            {
                window_pick = Some(super::window_picker::MatcherPath::Condition(path));
            }
        }
        MkAction::WaitUntil { condition, wait } => {
            if let Some(path) =
                super::condition_editor::condition_ui_with_assets(ui, condition, image_assets)
            {
                window_pick = Some(super::window_picker::MatcherPath::Condition(path));
            }
            ui.horizontal(|ui| {
                ui.label("Timeout (ms)");
                ui.add(egui::DragValue::new(&mut wait.timeout_ms).clamp_range(0..=86_400_000));
                ui.label("Poll (ms)");
                ui.add(
                    egui::DragValue::new(&mut wait.poll_interval_ms).clamp_range(1..=86_400_000),
                );
            });
        }
        MkAction::LauncherCommand { command, args } => {
            ui.horizontal(|ui| {
                ui.label("Canonical action");
                ui.text_edit_singleline(command);
            });
            let a = args.get_or_insert_with(String::new);
            ui.horizontal(|ui| {
                ui.label("Arguments");
                ui.text_edit_singleline(a);
            });
            if a.is_empty() {
                *args = None;
            }
            ui.small(
                "Search uses canonical launcher action values; display labels are not persisted.",
            );
            if ui.button("Choose Launcher Action…").clicked() {
                launcher_pick = Some(super::launcher_action_picker::PickerPurpose::LauncherCommand);
            }
        }
        MkAction::ImageFind(p) | MkAction::ImageClick(p) => {
            ui.heading("Reference Image");
            ui.horizontal(|ui| {
                if ui.button("Select PNG...").clicked() {
                    image_request = Some(ImageAuthoringRequest::Import);
                }
                if ui
                    .button("Capture...")
                    .on_hover_text(
                        "Capture the currently configured search region as the reference image",
                    )
                    .clicked()
                {
                    image_request = Some(ImageAuthoringRequest::Capture);
                }
            });
            ui.label(format!("mkmacro_assets/<macro>/{}.png", p.asset_id));
            ui.horizontal(|ui| {
                ui.label("Tolerance");
                ui.add(egui::Slider::new(&mut p.tolerance, 0..=255));
            });
            ui.horizontal(|ui| {
                ui.label("Return point");
                ui.selectable_value(&mut p.return_point, ReturnPoint::Center, "Center");
                ui.selectable_value(&mut p.return_point, ReturnPoint::TopLeft, "Top-left");
            });
            ui.horizontal(|ui| {
                ui.label("Search region");
                if ui
                    .selectable_label(matches!(p.region, SearchRegion::Desktop), "Entire Desktop")
                    .clicked()
                {
                    p.region = SearchRegion::Desktop;
                }
                if ui
                    .selectable_label(matches!(p.region, SearchRegion::Monitor { .. }), "Monitor")
                    .clicked()
                {
                    p.region = SearchRegion::Monitor { index: 0 };
                }
                if ui
                    .selectable_label(
                        matches!(p.region, SearchRegion::Rectangle { .. }),
                        "Rectangle",
                    )
                    .clicked()
                {
                    p.region = SearchRegion::Rectangle {
                        rect: ScreenRect::new(0, 0, 1, 1),
                    };
                }
                if ui
                    .selectable_label(matches!(p.region, SearchRegion::Window { .. }), "Window")
                    .clicked()
                {
                    p.region = SearchRegion::Window {
                        matcher: MkWindowMatcher::default(),
                    };
                }
                if ui
                    .selectable_label(
                        matches!(p.region, SearchRegion::ClientArea { .. }),
                        "Window Client Area",
                    )
                    .clicked()
                {
                    p.region = SearchRegion::ClientArea {
                        matcher: MkWindowMatcher::default(),
                    };
                }
            });
            if let SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } =
                &mut p.region
                && matcher_ui(ui, matcher)
            {
                window_pick = Some(super::window_picker::MatcherPath::ImageRegion);
            }
            if let SearchRegion::Monitor { index } = &mut p.region {
                ui.horizontal(|ui| {
                    ui.label("Zero-based monitor index");
                    ui.add(egui::DragValue::new(index));
                });
                ui.small("Monitor numbering is supplied by the production screen backend; an unavailable stored index is preserved.");
            }
            if let SearchRegion::Rectangle { rect } = &mut p.region {
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut rect.x));
                    ui.label("Y");
                    ui.add(egui::DragValue::new(&mut rect.y));
                });
                ui.horizontal(|ui| {
                    ui.label("Width");
                    ui.add(egui::DragValue::new(&mut rect.width).clamp_range(1..=u32::MAX));
                    ui.label("Height");
                    ui.add(egui::DragValue::new(&mut rect.height).clamp_range(1..=u32::MAX));
                });
            }
            ui.horizontal(|ui| {
                ui.label("Timeout (ms)");
                ui.add(egui::DragValue::new(&mut p.wait.timeout_ms));
                ui.label("Poll (ms)");
                ui.add(egui::DragValue::new(&mut p.wait.poll_interval_ms));
            });
        }
        MkAction::PixelCheck {
            target,
            color,
            tolerance,
        } => {
            ui.heading("Coordinate");
            let response = target_ui(ui, target);
            if response.pick_position {
                pick = Some(PositionCaptureSlot::PixelPosition);
            }
            if response.pick_matcher {
                window_pick = Some(super::window_picker::MatcherPath::Coordinate(
                    super::window_picker::CoordinateMatcherPath::PixelPosition,
                ));
            }
            ui.separator();
            ui.heading("Color");
            ui.horizontal(|ui| {
                ui.label("Color");
                let response = ui.text_edit_singleline(color);
                if response.lost_focus() {
                    if let Ok(rgb) = crate::mkmacro::screen::parse_rgb(color) {
                        *color = crate::mkmacro::screen::format_rgb(rgb);
                    }
                }
                match crate::mkmacro::screen::parse_rgb(color) {
                    Ok(rgb) => {
                        let swatch = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(28.0, 20.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 3.0, swatch);
                    }
                    Err(error) => {
                        ui.colored_label(egui::Color32::RED, error.to_string());
                    }
                }
                if ui.button("Pick Color").clicked() {
                    pick = Some(PositionCaptureSlot::PixelColor);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Tolerance");
                ui.add(egui::DragValue::new(tolerance).clamp_range(0..=255));
            });
        }
        _ => {
            ui.label(
                "This legacy action is unavailable for editing; its saved payload is preserved.",
            );
        }
    }
    (pick, window_pick, launcher_pick, image_request)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageAuthoringRequest {
    Import,
    Capture,
}

fn image_payload_mut(step: &mut MkStep) -> Option<&mut MkImagePayload> {
    match &mut step.action {
        MkAction::ImageFind(p) | MkAction::ImageClick(p) => Some(p),
        _ => None,
    }
}

fn apply_import(
    store: &MkMacroStore,
    macro_id: u64,
    payload: &mut MkImagePayload,
    source: &std::path::Path,
) -> anyhow::Result<()> {
    let id = store.next_asset_id(macro_id)?;
    store.import_png_asset(macro_id, id, source)?;
    payload.asset_id = id;
    Ok(())
}

fn apply_capture(
    store: &MkMacroStore,
    macro_id: u64,
    payload: &mut MkImagePayload,
    image: &image::RgbaImage,
) -> anyhow::Result<()> {
    let id = store.next_asset_id(macro_id)?;
    store.write_png_asset(macro_id, id, image)?;
    payload.asset_id = id;
    Ok(())
}

fn condition_at_path<'a>(
    mut c: &'a mut MkCondition,
    path: &[usize],
) -> Option<&'a mut MkWindowMatcher> {
    for &index in path {
        c = match c {
            MkCondition::All { conditions } | MkCondition::Any { conditions } => {
                conditions.get_mut(index)?
            }
            MkCondition::Not { condition } if index == 0 => condition,
            _ => return None,
        };
    }
    match c {
        MkCondition::WindowExists { matcher } | MkCondition::WindowActive { matcher } => {
            Some(matcher)
        }
        _ => None,
    }
}

fn matcher_at_path<'a>(
    action: &'a mut MkAction,
    path: &super::window_picker::MatcherPath,
) -> Option<&'a mut MkWindowMatcher> {
    use super::window_picker::MatcherPath;
    match path {
        MatcherPath::Action => match action {
            MkAction::WindowActivate(p) | MkAction::WindowWait(p) => Some(&mut p.matcher),
            MkAction::WindowClose(m) => Some(m),
            MkAction::WindowMoveResize(p) => Some(&mut p.matcher),
            MkAction::WindowState { matcher, .. } => Some(matcher),
            _ => None,
        },
        MatcherPath::Condition(path) => match action {
            MkAction::If(c)
            | MkAction::WhileStart { condition: c }
            | MkAction::WaitUntil { condition: c, .. } => condition_at_path(c, path),
            _ => None,
        },
        MatcherPath::ImageRegion => match action {
            MkAction::ImageFind(p) | MkAction::ImageClick(p) => match &mut p.region {
                SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } => {
                    Some(matcher)
                }
                _ => None,
            },
            _ => None,
        },
        MatcherPath::Coordinate(path) => {
            use super::window_picker::CoordinateMatcherPath::*;
            let target = match (path, action) {
                (MoveTarget, MkAction::MouseMove(p)) => &mut p.target,
                (ClickTarget, MkAction::MouseClick(p)) => &mut p.target,
                (DragFrom, MkAction::MouseDrag(p)) => &mut p.from,
                (DragTo, MkAction::MouseDrag(p)) => &mut p.to,
                (PixelPosition, MkAction::PixelCheck { target, .. }) => target,
                _ => return None,
            };
            match target {
                MkCoordinateTarget::WindowClient { matcher, .. } => Some(matcher),
                _ => None,
            }
        }
    }
}

pub(super) fn show(ctx: &egui::Context, d: &mut MkMacroDialog) {
    if d.action_editor.draft.is_none() {
        return;
    }
    let mut open = true;
    let mut apply = false;
    let mut cancel = false;
    let mut captured = None;
    if d.action_editor.position_capture.is_some() {
        ctx.request_repaint();
        match d.action_editor.picker.poll_event() {
            Ok(Some(event)) => d.action_editor.process_position_event(event),
            Ok(None) => {}
            Err(error) => {
                d.action_editor.stop_position_capture();
                d.action_editor.capture_message = Some(error);
            }
        }
    }
    let mut pick_request = None;
    let mut image_request = None;
    let image_assets = d
        .selected_macro_id
        .and_then(|id| d.store.asset_ids(id).ok())
        .unwrap_or_default();
    egui::Window::new("Action Editor")
        .open(&mut open)
        .collapsible(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            let state = &mut d.action_editor;
            let step = state.draft.as_mut().unwrap();
            let editor = state.editor.expect("action editor draft has no editor strategy");
            assert!(super::action_catalog::editor_route_recognizes(&step.action, editor), "action editor strategy does not match draft action");
            let (position, window, launcher, image)=action_ui(ui, step, &mut state.capture_keys, &image_assets);
            pick_request = position;
            image_request = image;
            if let Some(path)=window {
                let original=matcher_at_path(&mut step.action, &path).expect("picker path must resolve").clone();
                let macro_id=d.selected_macro_id.unwrap_or(0);
                d.window_picker.open(super::window_picker::MatcherEditRequest { destination: super::window_picker::MatcherDestination::Action { macro_id, draft_generation: state.draft_generation, path }, original });
            }
            if let Some(purpose) = launcher {
                let request = super::launcher_action_picker::LauncherActionRequest {
                    purpose,
                    macro_id: d.selected_macro_id.unwrap_or(0),
                    step_id: state.editing_id,
                    draft_generation: state.draft_generation,
                };
                d.launcher_action_picker.open(request);
            }
            if state.position_capture.is_some() {
                ui.colored_label(egui::Color32::YELLOW, "Move the mouse to the desired location. Left-click or press Enter to capture. Escape cancels.");
            }
            if let Some(payload)=image_payload_mut(step) {
                super::image_preview::show(ui, &d.store, d.selected_macro_id.unwrap_or(0), payload.asset_id);
            }
            if let Some(message) = &state.capture_message { ui.colored_label(egui::Color32::RED, message); }
            if state.capture_keys {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Press the next real key or chord. Escape cancels capture.",
                );
                if let Some(result) = ui.input(captured_chord) {
                    match result {
                        CapturedChord::Cancelled => state.capture_keys = false,
                        CapturedChord::Keys(keys) => captured = Some(keys),
                    }
                }
            }
            ui.separator();
            ui.heading("Step settings");
            ui.horizontal(|ui| {
                ui.label("Repeat");
                ui.add(egui::DragValue::new(&mut step.repeat).clamp_range(1..=1_000_000));
                ui.label("Delay after (ms)");
                ui.add(egui::DragValue::new(&mut step.delay_after_ms).clamp_range(0..=86_400_000));
            });
            egui::ComboBox::from_label("On error")
                .selected_text(match step.on_error {
                    MkErrorPolicy::Stop => "Stop",
                    MkErrorPolicy::Continue => "Continue",
                    MkErrorPolicy::Retry(_) => "Retry",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(matches!(step.on_error, MkErrorPolicy::Stop), "Stop")
                        .clicked()
                    {
                        step.on_error = MkErrorPolicy::Stop
                    }
                    if ui
                        .selectable_label(
                            matches!(step.on_error, MkErrorPolicy::Continue),
                            "Continue",
                        )
                        .clicked()
                    {
                        step.on_error = MkErrorPolicy::Continue
                    }
                    if ui
                        .selectable_label(matches!(step.on_error, MkErrorPolicy::Retry(_)), "Retry")
                        .clicked()
                    {
                        step.on_error = MkErrorPolicy::Retry(MkRetry {
                            attempts: 3,
                            delay_ms: 100,
                        })
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let valid = !matches!(&step.action, MkAction::PromptInput(p) if crate::mkmacro::variables::validate_variable_name(&p.variable).is_err());
                apply = ui.add_enabled(valid, egui::Button::new("Apply")).clicked();
                cancel = ui.button("Cancel").on_hover_text("Cancel editing; during playback Cancel stops the macro").clicked();
            });
        });
    if let Some(request) = image_request {
        let macro_id = d.selected_macro_id.unwrap_or(0);
        let result = match request {
            ImageAuthoringRequest::Import => rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .pick_file()
                .map(|path| {
                    apply_import(
                        &d.store,
                        macro_id,
                        image_payload_mut(d.action_editor.draft.as_mut().unwrap()).unwrap(),
                        &path,
                    )
                })
                .transpose(),
            ImageAuthoringRequest::Capture => {
                let region = image_payload_mut(d.action_editor.draft.as_mut().unwrap())
                    .unwrap()
                    .region
                    .clone();
                crate::mkmacro::WindowsScreenCaptureBackend::system()
                    .capture(&region, &|| false)
                    .map_err(anyhow::Error::msg)
                    .and_then(|captured| {
                        apply_capture(
                            &d.store,
                            macro_id,
                            image_payload_mut(d.action_editor.draft.as_mut().unwrap()).unwrap(),
                            &captured.image,
                        )
                    })
                    .map(Some)
            }
        };
        if let Err(error) = result {
            d.action_editor.capture_message = Some(format!("Reference image: {error:#}"));
        }
    }
    if let Some(slot) = pick_request {
        if let Err(error) = d.action_editor.start_position_capture(slot) {
            d.action_editor.capture_message = Some(error);
        }
    }
    if let Some(keys) = captured {
        d.action_editor.set_captured_keys(keys);
    }
    if apply {
        if let Some(MkStep {
            action: MkAction::PixelCheck { color, .. },
            ..
        }) = d.action_editor.draft.as_mut()
        {
            match crate::mkmacro::screen::parse_rgb(color) {
                Ok(rgb) => *color = crate::mkmacro::screen::format_rgb(rgb),
                Err(error) => {
                    d.action_editor.capture_message = Some(error.to_string());
                    return;
                }
            }
        }
        let mut state = std::mem::take(&mut d.action_editor);
        state.apply(d);
        d.action_editor = state;
        d.window_picker
            .cancel("Window picker closed because the action editor was applied");
        d.launcher_action_picker.cancel();
    } else if cancel || !open {
        d.action_editor.cancel();
        d.window_picker
            .cancel("Window picker closed because the action editor was cancelled");
        d.launcher_action_picker.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    fn step(a: MkAction) -> MkStep {
        MkStep {
            id: 7,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: a,
        }
    }

    #[test]
    fn image_asset_updates_are_transactional() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let mut payload = MkImagePayload {
            asset_id: 9,
            wait: MkWaitOptions {
                timeout_ms: 5_000,
                poll_interval_ms: 100,
            },
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
        };
        let bad = dir.path().join("bad.png");
        std::fs::write(&bad, b"not png").unwrap();
        assert!(apply_import(&store, 4, &mut payload, &bad).is_err());
        assert_eq!(payload.asset_id, 9);

        let image = RgbaImage::from_pixel(3, 2, Rgba([1, 2, 3, 255]));
        apply_capture(&store, 4, &mut payload, &image).unwrap();
        assert_eq!(payload.asset_id, 1);
        assert_eq!(
            image::open(store.asset_path(4, 1).unwrap())
                .unwrap()
                .width(),
            3
        );
    }
    fn capture(slot: PositionCaptureSlot) -> PositionCaptureState {
        PositionCaptureState {
            target_step_id: Some(7),
            draft_generation: 1,
            slot,
            awaiting_release: true,
            last_screen_position: None,
            #[cfg(windows)]
            foreground_window: 0,
        }
    }

    #[test]
    fn window_picker_routes_root_nested_and_image_matchers() {
        let replacement = MkWindowMatcher {
            process: Some("picked.exe".into()),
            title: Some("Picked".into()),
            title_regex: None,
            class: None,
        };
        for (action, path) in [
            (
                MkAction::WindowClose(MkWindowMatcher::default()),
                super::super::window_picker::MatcherPath::Action,
            ),
            (
                MkAction::WhileStart {
                    condition: MkCondition::All {
                        conditions: vec![MkCondition::WindowExists {
                            matcher: MkWindowMatcher::default(),
                        }],
                    },
                },
                super::super::window_picker::MatcherPath::Condition(vec![0]),
            ),
            (
                MkAction::ImageFind(MkImagePayload {
                    asset_id: 1,
                    tolerance: 0,
                    alpha: AlphaPolicy::Compare,
                    region: SearchRegion::Window {
                        matcher: MkWindowMatcher::default(),
                    },
                    return_point: ReturnPoint::Center,
                    wait: MkWaitOptions {
                        timeout_ms: 0,
                        poll_interval_ms: 1,
                    },
                }),
                super::super::window_picker::MatcherPath::ImageRegion,
            ),
        ] {
            let mut editor = ActionEditorState::default();
            editor.begin_edit(&step(action));
            let request = super::super::window_picker::MatcherEditRequest {
                destination: super::super::window_picker::MatcherDestination::Action {
                    macro_id: 5,
                    draft_generation: editor.draft_generation,
                    path: path.clone(),
                },
                original: MkWindowMatcher::default(),
            };
            assert!(editor.apply_window_matcher(&request, replacement.clone(), Some(5)));
            assert_eq!(
                matcher_at_path(&mut editor.draft.as_mut().unwrap().action, &path),
                Some(&mut replacement.clone())
            );
            assert!(
                !editor.apply_window_matcher(&request, MkWindowMatcher::default(), Some(6)),
                "another macro must not be modified"
            );
        }
    }

    #[test]
    fn capture_is_frame_driven_armed_and_one_shot() {
        let mut e = ActionEditorState::default();
        e.begin_edit(&step(MkAction::MouseMove(MkMouseMovePayload {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: 1, y: 2 },
            },
            duration_ms: 0,
        })));
        e.draft_generation = 1;
        e.position_capture = Some(capture(PositionCaptureSlot::MoveTarget));
        e.process_position_event(PositionCaptureEvent::PointerMoved(MkPoint {
            x: -300,
            y: 44,
        }));
        e.process_position_event(PositionCaptureEvent::LeftPressed);
        assert!(e.position_capture.is_some(), "initiating press is ignored");
        e.process_position_event(PositionCaptureEvent::LeftReleased);
        assert!(!e.position_capture.as_ref().unwrap().awaiting_release);
        e.process_position_event(PositionCaptureEvent::LeftPressed);
        assert!(e.position_capture.is_none());
        let MkAction::MouseMove(p) = &e.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(
            p.target,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: -300, y: 44 }
            }
        );
        e.process_position_event(PositionCaptureEvent::LeftPressed);
        let MkAction::MouseMove(p) = &e.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(
            p.target,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: -300, y: 44 }
            }
        );
    }

    #[test]
    fn escape_and_editor_lifecycle_clear_capture_without_mutation() {
        let source = step(MkAction::MouseClick(MkMousePayload {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: 9, y: 8 },
            },
            button: MkMouseButton::Left,
            clicks: 1,
        }));
        let mut e = ActionEditorState::default();
        e.begin_edit(&source);
        e.draft_generation = 1;
        e.position_capture = Some(capture(PositionCaptureSlot::ClickTarget));
        e.process_position_event(PositionCaptureEvent::Escape);
        assert!(e.position_capture.is_none());
        assert_eq!(e.draft.as_ref().unwrap().action, source.action);
        e.position_capture = Some(capture(PositionCaptureSlot::ClickTarget));
        e.cancel();
        assert!(e.position_capture.is_none());
    }

    #[test]
    fn desktop_color_pick_updates_only_after_confirmation_and_escape_preserves_draft() {
        let source = step(MkAction::PixelCheck {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: -8, y: 4 },
            },
            color: "#112233".into(),
            tolerance: 0,
        });
        let mut editor = ActionEditorState::default();
        editor.begin_edit(&source);
        editor.draft_generation = 1;
        let mut pending = capture(PositionCaptureSlot::PixelColor);
        pending.last_screen_position = Some(MkPoint { x: -8, y: 4 });
        editor.confirm_position_with(pending, |point| {
            assert_eq!(point, MkPoint { x: -8, y: 4 });
            Ok([0xab, 0, 0xff])
        });
        let MkAction::PixelCheck { color, .. } = &editor.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(color, "#AB00FF");

        editor.position_capture = Some(capture(PositionCaptureSlot::PixelColor));
        editor.process_position_event(PositionCaptureEvent::Escape);
        let MkAction::PixelCheck { color, .. } = &editor.draft.as_ref().unwrap().action else {
            panic!()
        };
        assert_eq!(color, "#AB00FF");
    }

    #[test]
    fn drag_slots_are_independent() {
        let drag = MkMouseDragPayload {
            from: MkCoordinateTarget::Screen {
                point: MkPoint { x: 1, y: 2 },
            },
            to: MkCoordinateTarget::Screen {
                point: MkPoint { x: 3, y: 4 },
            },
            button: MkMouseButton::X2,
            duration_ms: 10,
        };
        let mut e = ActionEditorState::default();
        e.begin_edit(&step(MkAction::MouseDrag(drag)));
        e.draft_generation = 1;
        let mut c = capture(PositionCaptureSlot::DragFrom);
        c.awaiting_release = false;
        c.last_screen_position = Some(MkPoint { x: -5, y: -6 });
        e.position_capture = Some(c);
        e.process_position_event(PositionCaptureEvent::Enter);
        let MkAction::MouseDrag(p) = &e.draft.unwrap().action else {
            panic!()
        };
        assert_eq!(
            p.from,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: -5, y: -6 }
            }
        );
        assert_eq!(
            p.to,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: 3, y: 4 }
            }
        );
    }
    #[test]
    fn capture_chooses_press_hotkey_down_and_up() {
        let mut e = ActionEditorState::default();
        e.begin_edit(&step(MkAction::KeyPress(MkKey::Enter)));
        assert!(e.set_captured_keys(vec![MkKey::Character("A".into())]));
        assert!(matches!(
            e.draft.as_ref().unwrap().action,
            MkAction::KeyPress(_)
        ));
        e.set_captured_keys(vec![MkKey::Control, MkKey::Character("K".into())]);
        assert!(matches!(
            e.draft.as_ref().unwrap().action,
            MkAction::Hotkey(_)
        ));
        e.begin_edit(&step(MkAction::KeyDown(MkKey::Enter)));
        e.set_captured_keys(vec![MkKey::Control, MkKey::Character("Q".into())]);
        assert!(matches!(
            e.draft.as_ref().unwrap().action,
            MkAction::KeyDown(MkKey::Character(_))
        ));
        e.begin_edit(&step(MkAction::KeyUp(MkKey::Enter)));
        e.set_captured_keys(vec![MkKey::Character("Q".into())]);
        assert!(matches!(
            e.draft.as_ref().unwrap().action,
            MkAction::KeyUp(_)
        ));
    }
    #[test]
    fn cancel_does_not_touch_source() {
        let source = step(MkAction::Delay { milliseconds: 12 });
        let bytes = serde_json::to_vec(&source).unwrap();
        let mut e = ActionEditorState::default();
        e.begin_edit(&source);
        if let Some(MkStep {
            action: MkAction::Delay { milliseconds },
            ..
        }) = e.draft.as_mut()
        {
            *milliseconds = 99
        }
        e.cancel();
        assert_eq!(serde_json::to_vec(&source).unwrap(), bytes);
    }
    #[test]
    fn screen_and_window_relative_position_are_pure() {
        let mut t = MkCoordinateTarget::Screen {
            point: MkPoint { x: 0, y: 0 },
        };
        apply_picked_position(&mut t, MkPoint { x: -20, y: 40 }, None).unwrap();
        assert_eq!(
            t,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: -20, y: 40 }
            }
        );
        let w = PickedWindow {
            matcher: MkWindowMatcher {
                title: Some("Editor".into()),
                title_regex: None,
                process: Some("app.exe".into()),
                class: Some("Class".into()),
            },
            client_origin: MkPoint { x: -100, y: 10 },
        };
        let mut t = MkCoordinateTarget::ActiveWindow {
            point: MkPoint { x: 0, y: 0 },
        };
        apply_picked_position(&mut t, MkPoint { x: -20, y: 40 }, Some(&w)).unwrap();
        assert_eq!(
            t,
            MkCoordinateTarget::ActiveWindow {
                point: MkPoint { x: 80, y: 30 }
            }
        );
    }
    #[test]
    fn picked_window_stores_only_durable_matcher() {
        let mut p = MkWindowPayload {
            matcher: MkWindowMatcher {
                title: None,
                title_regex: None,
                process: None,
                class: None,
            },
            wait: None,
        };
        let picked = PickedWindow {
            matcher: MkWindowMatcher {
                title: Some("Document".into()),
                title_regex: None,
                process: Some("editor.exe".into()),
                class: Some("EditorWindow".into()),
            },
            client_origin: MkPoint { x: 1, y: 2 },
        };
        apply_picked_window(&mut p, &picked);
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("editor.exe"));
        assert!(!json.to_lowercase().contains("hwnd"));
    }
}
