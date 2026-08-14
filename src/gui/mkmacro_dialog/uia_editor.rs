//! Platform-neutral state for the UIA picker and recorded-click conversion UI.
use crate::mkmacro::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, MkAction, MkPoint, MkUiPattern, MkUiPayload,
    UiElementInfo, require_pattern, validate_selector,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Capturing { cursor: MkPoint },
    Preview(Box<UiElementInfo>),
}
#[derive(Debug, Clone)]
pub struct UiaEditorState {
    pub capture: CaptureState,
    pub candidate: Option<UiElementInfo>,
    pub conversion_step: Option<u64>,
}
impl Default for UiaEditorState {
    fn default() -> Self {
        Self {
            capture: CaptureState::Idle,
            candidate: None,
            conversion_step: None,
        }
    }
}
impl UiaEditorState {
    pub fn editor_hidden(&self) -> bool {
        !matches!(self.capture, CaptureState::Idle)
    }
    pub fn begin_pick(&mut self, cursor: MkPoint) {
        self.candidate = None;
        self.capture = CaptureState::Capturing { cursor };
    }
    pub fn preview(&mut self, info: UiElementInfo) {
        self.candidate = Some(info.clone());
        self.capture = CaptureState::Preview(Box::new(info));
    }
    pub fn cancel(&mut self) {
        self.capture = CaptureState::Idle;
        self.candidate = None;
        self.conversion_step = None;
    }
    pub fn confirm(&mut self) -> ExecResult<UiElementInfo> {
        let info = self.candidate.take().ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidSelection,
                "no UI control is selected",
            )
        })?;
        validate_selector(&info.selector)?;
        self.capture = CaptureState::Idle;
        Ok(info)
    }
    pub fn begin_recorded_click_conversion(&mut self, step: u64, info: UiElementInfo) {
        self.conversion_step = Some(step);
        self.preview(info);
    }
    /// Starts the mouse editor's "Find UI Control At Recorded Position" flow without modifying
    /// the recorded action.
    pub fn find_at_recorded_position(&mut self, step: u64, point: MkPoint) {
        self.conversion_step = Some(step);
        self.begin_pick(point);
    }
    /// The caller replaces exactly `step_id` only after this returns a validated draft.
    pub fn conversion_draft(&self, step_id: u64, target: MkUiPayload) -> ExecResult<MkAction> {
        if self.conversion_step != Some(step_id) {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidSelection,
                "recorded click conversion no longer targets this step",
            ));
        }
        let info = self.candidate.as_ref().ok_or_else(|| {
            ExecutionDiagnostic::new(DiagnosticKind::InvalidSelection, "no conversion candidate")
        })?;
        require_pattern(info, MkUiPattern::Invoke)?;
        validate_selector(&target.selector)?;
        Ok(MkAction::UiInvoke(target))
    }
}

pub(super) fn show(ui: &mut eframe::egui::Ui, state: &mut UiaEditorState) {
    match &state.capture {
        CaptureState::Idle => {
            if ui.button("Pick UI Control").clicked() {
                let p = ui.ctx().pointer_latest_pos().unwrap_or_default();
                state.begin_pick(MkPoint {
                    x: p.x as i32,
                    y: p.y as i32,
                });
            }
            ui.label("Mouse clicks remain physical unless explicitly converted.");
        }
        CaptureState::Capturing { .. } => {
            ui.label("UI control capture active — move the pointer, then confirm or press Escape to cancel.");
        }
        CaptureState::Preview(info) => {
            if let Some((x, y, width, height)) = info.bounds {
                ui.ctx().debug_painter().rect_stroke(
                    eframe::egui::Rect::from_min_size(
                        eframe::egui::pos2(x as f32, y as f32),
                        eframe::egui::vec2(width as f32, height as f32),
                    ),
                    2.0,
                    eframe::egui::Stroke::new(3.0_f32, eframe::egui::Color32::YELLOW),
                );
            }
            ui.group(|ui| {
                ui.heading("UI control selector preview");
                ui.label(format!("Name: {}", info.user_facing_name));
                ui.label(format!(
                    "Automation ID: {}",
                    info.selector.automation_id.as_deref().unwrap_or("—")
                ));
                ui.label(format!("Type: {:?}", info.selector.control_type));
                ui.label(format!(
                    "Class: {}",
                    info.selector.class_name.as_deref().unwrap_or("—")
                ));
                ui.label(format!("Executable: {}", info.target_executable));
                ui.label(format!(
                    "Supported operations: {:?}",
                    info.supported_patterns
                ));
                if state.conversion_step.is_some()
                    && info.supported_patterns.contains(&MkUiPattern::Invoke)
                {
                    ui.label("Convert to UI Automation Invoke (confirmation required)");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkUiControlType, MkUiSelector, MkWindowMatcher};
    use std::collections::HashSet;
    fn info(invoke: bool) -> UiElementInfo {
        UiElementInfo {
            selector: MkUiSelector {
                automation_id: Some("x".into()),
                name: None,
                control_type: Some(MkUiControlType::Button),
                class_name: None,
                framework_id: None,
                ancestor_path: vec![],
            },
            user_facing_name: "X".into(),
            target_executable: "x.exe".into(),
            supported_patterns: if invoke {
                [MkUiPattern::Invoke].into_iter().collect()
            } else {
                HashSet::new()
            },
            bounds: None,
        }
    }
    fn payload() -> MkUiPayload {
        MkUiPayload {
            window: MkWindowMatcher {
                title: None,
                title_regex: None,
                process: Some("x.exe".into()),
                class: None,
            },
            selector: info(true).selector,
            wait: None,
        }
    }
    #[test]
    fn inspector_cancel_and_confirm() {
        let mut s = UiaEditorState::default();
        s.begin_pick(MkPoint { x: 1, y: 2 });
        assert!(s.editor_hidden());
        s.preview(info(true));
        assert_eq!(s.confirm().unwrap().user_facing_name, "X");
        s.begin_pick(MkPoint { x: 0, y: 0 });
        s.cancel();
        assert_eq!(s.capture, CaptureState::Idle);
    }
    #[test]
    fn conversion_is_explicit_and_preserves_unrelated_action() {
        let original = MkAction::MouseScroll { i32_delta: 1 };
        let mut s = UiaEditorState::default();
        s.begin_recorded_click_conversion(7, info(true));
        assert!(matches!(
            s.conversion_draft(7, payload()).unwrap(),
            MkAction::UiInvoke(_)
        ));
        assert_eq!(original, MkAction::MouseScroll { i32_delta: 1 });
        assert!(s.conversion_draft(8, payload()).is_err());
    }
    #[test]
    fn conversion_requires_invoke() {
        let mut s = UiaEditorState::default();
        s.begin_recorded_click_conversion(7, info(false));
        assert_eq!(
            s.conversion_draft(7, payload()).unwrap_err().kind,
            DiagnosticKind::UnsupportedPattern
        );
    }
}
