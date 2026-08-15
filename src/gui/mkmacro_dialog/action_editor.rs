//! Transactional, typed action editing.
//!
//! The editor owns a complete `MkStep` clone.  No document field is borrowed by
//! the modal, which makes closing/cancelling it a genuinely lossless operation.
use super::{
    MkMacroDialog,
    key_capture::{CapturedChord, captured_chord, key_name},
};
use crate::mkmacro::variables::MkPoint;
use crate::mkmacro::*;
use eframe::egui;

#[derive(Default)]
pub struct ActionEditorState {
    pub draft: Option<MkStep>,
    /// `None` means insert a new row; otherwise replace this stable step id.
    pub editing_id: Option<u64>,
    pub capture_keys: bool,
    pub capture_message: Option<String>,
    pub editor: Option<super::action_catalog::EditorKind>,
    position_capture: Option<PositionCaptureState>,
    draft_generation: u64,
    picker: NativePositionPicker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PositionCaptureSlot {
    MoveTarget,
    ClickTarget,
    DragFrom,
    DragTo,
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

#[derive(Default)]
pub struct QuickInsertState {
    pub keys: Vec<MkKey>,
    pub repeat: u32,
    pub delay_after_ms: u64,
    pub capturing: bool,
}
impl QuickInsertState {
    pub fn action(&self) -> Option<MkAction> {
        let primary = self.keys.iter().rposition(|k| !is_modifier(k))?;
        if self.keys.len() == 1 {
            Some(MkAction::KeyPress(self.keys[primary].clone()))
        } else {
            Some(MkAction::Hotkey(self.keys.clone()))
        }
    }
    pub fn insert(&mut self, d: &mut MkMacroDialog) -> Option<u64> {
        let action = self.action()?;
        let repeat = self.repeat.max(1);
        let delay = self.delay_after_ms;
        let selected = d.selection.ids.clone();
        let m = d.selected_macro_mut()?;
        let pos = m
            .steps
            .iter()
            .rposition(|s| selected.contains(&s.id))
            .map_or(m.steps.len(), |i| i + 1);
        m.steps.insert(
            pos,
            MkStep {
                id: 0,
                enabled: true,
                repeat,
                delay_after_ms: delay,
                on_error: MkErrorPolicy::Stop,
                action,
            },
        );
        repair_ids(&mut d.draft);
        let id = d.selected_macro()?.steps[pos].id;
        d.selection.ids.clear();
        d.selection.ids.insert(id);
        d.mark_dirty();
        self.keys.clear();
        Some(id)
    }
}

impl ActionEditorState {
    pub fn begin_new(&mut self, action: MkAction) {
        self.begin_new_with_editor(action, super::action_catalog::EditorKind::Generic);
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
        self.draft = Some(step.clone());
        self.editor = Some(super::action_catalog::editor_for_action(&step.action));
    }
    pub fn cancel(&mut self) {
        self.stop_position_capture();
        self.draft = None;
        self.editing_id = None;
        self.capture_keys = false;
        self.editor = None;
    }
    pub fn apply(&mut self, dialog: &mut MkMacroDialog) -> Option<u64> {
        self.stop_position_capture();
        let mut step = self.draft.take()?;
        let edit = self.editing_id.take();
        self.editor = None;
        let selected = dialog.selection.ids.clone();
        let m = dialog.selected_macro_mut()?;
        let index = if let Some(id) = edit {
            let i = m.steps.iter().position(|s| s.id == id)?;
            step.id = id;
            m.steps[i] = step;
            i
        } else {
            step.id = 0;
            let i = m
                .steps
                .iter()
                .rposition(|s| selected.contains(&s.id))
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
        let Some(screen) = capture.last_screen_position else {
            self.capture_message = Some("Unable to read the current pointer position".into());
            return;
        };
        let window = picked_foreground_window(&capture);
        let result = match self.target_mut(capture.slot) {
            Some(target @ MkCoordinateTarget::ActiveWindow { .. }) => {
                window.and_then(|w| apply_picked_position(target, screen, Some(&w)))
            }
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
            *point = MkPoint {
                x: screen.x - w.client_origin.x,
                y: screen.y - w.client_origin.y,
            };
        }
        _ => return Err("This target does not store a fixed position".into()),
    }
    Ok(())
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
fn matcher_ui(ui: &mut egui::Ui, m: &mut MkWindowMatcher) {
    optional_field(ui, "Executable/process", &mut m.process);
    optional_field(ui, "Title contains", &mut m.title);
    optional_field(ui, "Title regex", &mut m.title_regex);
    optional_field(ui, "Window class", &mut m.class);
    ui.add_enabled(
        false,
        egui::Button::new("Pick window (unavailable on this platform/session)"),
    );
}
fn target_ui(ui: &mut egui::Ui, target: &mut MkCoordinateTarget) -> bool {
    let kind = match target {
        MkCoordinateTarget::Screen { .. } => 0,
        MkCoordinateTarget::ActiveWindow { .. } => 1,
        MkCoordinateTarget::Variable { .. } => 2,
        MkCoordinateTarget::Image { .. } => 3,
    };
    let mut next = kind;
    egui::ComboBox::from_label("Target")
        .selected_text(["Screen", "Active Window", "Variable", "Image Result"][kind])
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut next, 0, "Screen");
            ui.selectable_value(&mut next, 1, "Active Window");
            ui.selectable_value(&mut next, 2, "Variable");
            ui.selectable_value(&mut next, 3, "Image Result");
        });
    if next != kind {
        *target = match next {
            0 => MkCoordinateTarget::Screen {
                point: MkPoint { x: 0, y: 0 },
            },
            1 => MkCoordinateTarget::ActiveWindow {
                point: MkPoint { x: 0, y: 0 },
            },
            2 => MkCoordinateTarget::Variable {
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
            return picked;
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
    false
}

fn action_ui(
    ui: &mut egui::Ui,
    step: &mut MkStep,
    capture: &mut bool,
) -> Option<PositionCaptureSlot> {
    let mut pick = None;
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
            if target_ui(ui, &mut p.target) {
                pick = Some(PositionCaptureSlot::MoveTarget);
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
            if target_ui(ui, &mut p.from) {
                pick = Some(PositionCaptureSlot::DragFrom);
            }
            ui.label("Destination");
            if target_ui(ui, &mut p.to) {
                pick = Some(PositionCaptureSlot::DragTo);
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
            if target_ui(ui, &mut p.target) {
                pick = Some(PositionCaptureSlot::ClickTarget);
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
            });
            if wd.is_empty() {
                p.working_directory = None;
            }
            ui.checkbox(&mut p.wait, "Wait for completion");
        }
        MkAction::WindowActivate(p) | MkAction::WindowWait(p) => {
            matcher_ui(ui, &mut p.matcher);
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
        MkAction::WindowClose(m) => matcher_ui(ui, m),
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
        }
        MkAction::ImageFind(p) | MkAction::ImageClick(p) => {
            ui.horizontal(|ui| {
                ui.label("Image asset ID");
                ui.add(egui::DragValue::new(&mut p.asset_id));
            });
            ui.horizontal(|ui| {
                ui.label("Confidence");
                ui.add(egui::Slider::new(&mut p.confidence, 0.0..=1.0));
            });
            ui.horizontal(|ui| {
                ui.label("Timeout (ms)");
                ui.add(egui::DragValue::new(&mut p.wait.timeout_ms));
                ui.label("Poll (ms)");
                ui.add(egui::DragValue::new(&mut p.wait.poll_interval_ms));
            });
            ui.small("Search scope: screen");
        }
        MkAction::PixelCheck {
            target,
            color,
            tolerance,
        } => {
            let _ = target_ui(ui, target);
            ui.horizontal(|ui| {
                ui.label("Color");
                ui.text_edit_singleline(color);
                ui.label("Tolerance");
                ui.add(egui::DragValue::new(tolerance));
            });
        }
        _ => {
            ui.label(
                "This legacy action is unavailable for editing; its saved payload is preserved.",
            );
        }
    }
    pick
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
    egui::Window::new("Action Editor")
        .open(&mut open)
        .collapsible(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            let state = &mut d.action_editor;
            let step = state.draft.as_mut().unwrap();
            pick_request = action_ui(ui, step, &mut state.capture_keys);
            if state.position_capture.is_some() {
                ui.colored_label(egui::Color32::YELLOW, "Move the mouse to the desired location. Left-click or press Enter to capture. Escape cancels.");
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
                apply = ui.button("Apply").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if let Some(slot) = pick_request {
        if let Err(error) = d.action_editor.start_position_capture(slot) {
            d.action_editor.capture_message = Some(error);
        }
    }
    if let Some(keys) = captured {
        d.action_editor.set_captured_keys(keys);
    }
    if apply {
        let mut state = std::mem::take(&mut d.action_editor);
        state.apply(d);
        d.action_editor = state;
    } else if cancel || !open {
        d.action_editor.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
