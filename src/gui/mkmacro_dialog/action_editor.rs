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
        self.editing_id = None;
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
        self.editing_id = Some(step.id);
        self.draft = Some(step.clone());
    }
    pub fn cancel(&mut self) {
        self.draft = None;
        self.editing_id = None;
        self.capture_keys = false;
    }
    pub fn apply(&mut self, dialog: &mut MkMacroDialog) -> Option<u64> {
        let mut step = self.draft.take()?;
        let edit = self.editing_id.take();
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

/// Injectable boundary for native pointer capture. Values are virtual-desktop
/// screen coordinates, never egui-local positions.
pub trait PositionPicker {
    fn pick_screen_position(&mut self) -> Result<Option<MkPoint>, String>;
}
#[derive(Clone, Debug, PartialEq)]
pub struct PickedWindow {
    pub matcher: MkWindowMatcher,
    pub client_origin: MkPoint,
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
fn target_ui(ui: &mut egui::Ui, target: &mut MkCoordinateTarget) {
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
            ui.add_enabled(false, egui::Button::new("Capture position (unavailable)"));
            ui.small("Move the pointer, then click or press Enter to capture. Escape cancels.");
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
}

fn action_ui(ui: &mut egui::Ui, step: &mut MkStep, capture: &mut bool) {
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
        MkAction::MouseMove(t) => target_ui(ui, t),
        MkAction::MouseClick(p) => {
            target_ui(ui, &mut p.target);
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
        _ => {
            ui.label("This action uses its existing specialized editor.");
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
    egui::Window::new("Action Editor")
        .open(&mut open)
        .collapsible(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            let state = &mut d.action_editor;
            let step = state.draft.as_mut().unwrap();
            action_ui(ui, step, &mut state.capture_keys);
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
