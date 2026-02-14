use crate::draw::history::DrawHistory;
use crate::draw::keyboard_hook::{map_key_event_to_command, KeyCommand, KeyEvent};
use crate::draw::messages::ExitReason;
use crate::draw::model::{DrawObject, Geometry, ObjectStyle, Tool};
use crate::draw::runtime;

const MIN_POINT_DIST_SQ: i64 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointerModifiers {
    pub ctrl: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputCommand {
    Undo,
    Redo,
    RequestExit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawInputState {
    tool: Tool,
    style: ObjectStyle,
    active_geometry: Option<Geometry>,
    history: DrawHistory,
}

impl DrawInputState {
    pub fn new(tool: Tool, style: ObjectStyle) -> Self {
        Self {
            tool,
            style,
            active_geometry: None,
            history: DrawHistory::default(),
        }
    }

    pub fn history(&self) -> &DrawHistory {
        &self.history
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
    }

    pub fn set_style(&mut self, style: ObjectStyle) {
        self.style = style;
    }

    pub fn canvas_with_active(&self) -> crate::draw::model::CanvasModel {
        let mut canvas = self.history.canvas();
        if let Some(active) = &self.active_geometry {
            canvas.objects.push(DrawObject {
                tool: self.tool,
                style: self.style,
                geometry: active.clone(),
            });
        }
        canvas
    }

    pub fn handle_left_down(
        &mut self,
        point: (i32, i32),
        modifiers: PointerModifiers,
    ) -> Option<InputCommand> {
        if modifiers.shift || modifiers.ctrl {
            return Some(InputCommand::RequestExit);
        }

        self.active_geometry = Some(match self.tool {
            Tool::Pen => Geometry::Pen {
                points: vec![point],
            },
            Tool::Line => Geometry::Line {
                start: point,
                end: point,
            },
            Tool::Rect => Geometry::Rect {
                start: point,
                end: point,
            },
            Tool::Ellipse => Geometry::Ellipse {
                start: point,
                end: point,
            },
            Tool::Eraser => Geometry::Eraser {
                points: vec![point],
            },
        });

        None
    }

    pub fn handle_move(&mut self, point: (i32, i32)) {
        match self.active_geometry.as_mut() {
            Some(Geometry::Pen { points }) | Some(Geometry::Eraser { points }) => {
                if should_append_point(points.last().copied(), point) {
                    points.push(point);
                }
            }
            Some(Geometry::Line { end, .. })
            | Some(Geometry::Rect { end, .. })
            | Some(Geometry::Ellipse { end, .. }) => {
                *end = point;
            }
            None => {}
        }
    }

    pub fn handle_left_up(&mut self, point: (i32, i32)) {
        self.handle_move(point);
        if let Some(geometry) = self.active_geometry.take() {
            self.history.commit(DrawObject {
                tool: self.tool,
                style: self.style,
                geometry,
            });
        }
    }

    pub fn handle_key_event(&mut self, event: KeyEvent) -> Option<InputCommand> {
        match map_key_event_to_command(true, event) {
            Some(KeyCommand::Undo) => {
                let _ = self.history.undo();
                Some(InputCommand::Undo)
            }
            Some(KeyCommand::Redo) => {
                let _ = self.history.redo();
                Some(InputCommand::Redo)
            }
            Some(KeyCommand::RequestExit) => Some(InputCommand::RequestExit),
            None => None,
        }
    }

    pub fn request_exit(&self) -> InputCommand {
        InputCommand::RequestExit
    }
}

pub fn route_command_to_runtime(command: Option<InputCommand>, exit_reason: ExitReason) {
    route_command(command, exit_reason, |reason| {
        if let Err(err) = runtime().request_exit(reason) {
            tracing::warn!(?err, "failed to request draw exit from input command");
        }
    });
}

fn route_command<F>(command: Option<InputCommand>, exit_reason: ExitReason, mut request_exit: F)
where
    F: FnMut(ExitReason),
{
    if matches!(command, Some(InputCommand::RequestExit)) {
        request_exit(exit_reason);
    }
}

pub fn bridge_left_down_to_runtime(
    state: &mut DrawInputState,
    point: (i32, i32),
    modifiers: PointerModifiers,
) {
    route_command_to_runtime(
        state.handle_left_down(point, modifiers),
        ExitReason::UserRequest,
    );
}

pub fn bridge_mouse_move_to_runtime(state: &mut DrawInputState, point: (i32, i32)) {
    state.handle_move(point);
}

pub fn bridge_left_up_to_runtime(state: &mut DrawInputState, point: (i32, i32)) {
    state.handle_left_up(point);
}

pub fn bridge_key_event_to_runtime(state: &mut DrawInputState, event: KeyEvent) {
    route_command_to_runtime(state.handle_key_event(event), ExitReason::UserRequest);
}

fn should_append_point(last: Option<(i32, i32)>, point: (i32, i32)) -> bool {
    let Some((last_x, last_y)) = last else {
        return true;
    };

    let dx = point.0 as i64 - last_x as i64;
    let dy = point.1 as i64 - last_y as i64;
    dx * dx + dy * dy >= MIN_POINT_DIST_SQ
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::keyboard_hook::{KeyCode, KeyModifiers};

    fn draw_state(tool: Tool) -> DrawInputState {
        DrawInputState::new(tool, ObjectStyle::default())
    }

    #[test]
    fn pen_commit_creates_single_history_object() {
        let mut state = draw_state(Tool::Pen);

        assert_eq!(
            state.handle_left_down((10, 10), PointerModifiers::default()),
            None
        );
        state.handle_move((10, 11));
        state.handle_move((14, 14));
        state.handle_left_up((18, 18));

        assert_eq!(state.history().undo_len(), 1);
        assert_eq!(state.history().canvas().objects.len(), 1);
    }

    #[test]
    fn every_tool_commits_single_object_on_left_down_move_up() {
        for tool in [
            Tool::Pen,
            Tool::Line,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Eraser,
        ] {
            let mut state = draw_state(tool);
            assert_eq!(
                state.handle_left_down((3, 4), PointerModifiers::default()),
                None
            );
            state.handle_move((9, 10));
            state.handle_left_up((12, 14));

            assert_eq!(
                state.history().undo_len(),
                1,
                "unexpected undo count for {tool:?}"
            );
            assert_eq!(
                state.history().canvas().objects.len(),
                1,
                "unexpected object count for {tool:?}"
            );
        }
    }

    #[test]
    fn pen_and_eraser_respect_movement_threshold() {
        for tool in [Tool::Pen, Tool::Eraser] {
            let mut state = draw_state(tool);
            let _ = state.handle_left_down((0, 0), PointerModifiers::default());
            state.handle_move((1, 1));
            state.handle_move((2, 2));
            state.handle_left_up((3, 0));

            let history = state.history();
            let canvas = history.canvas();
            let object = canvas.objects.first().expect("single committed object");
            match &object.geometry {
                Geometry::Pen { points } | Geometry::Eraser { points } => {
                    assert_eq!(
                        points,
                        &vec![(0, 0), (3, 0)],
                        "unexpected points for {tool:?}"
                    );
                }
                other => panic!("unexpected geometry for {tool:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn shift_click_requests_exit() {
        let mut state = draw_state(Tool::Line);
        let command = state.handle_left_down(
            (0, 0),
            PointerModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(command, Some(InputCommand::RequestExit));
        assert_eq!(state.history().undo_len(), 0);
    }

    #[test]
    fn ctrl_click_requests_exit() {
        let mut state = draw_state(Tool::Rect);
        let command = state.handle_left_down(
            (0, 0),
            PointerModifiers {
                shift: false,
                ctrl: true,
            },
        );
        assert_eq!(command, Some(InputCommand::RequestExit));
        assert_eq!(state.history().undo_len(), 0);
    }

    #[test]
    fn esc_requests_exit() {
        let mut state = draw_state(Tool::Ellipse);
        let command = state.handle_key_event(KeyEvent {
            key: KeyCode::Escape,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(command, Some(InputCommand::RequestExit));
    }

    #[test]
    fn u_undo_and_ctrl_r_redo_shortcuts_dispatch_correct_commands() {
        let mut state = draw_state(Tool::Line);

        let _ = state.handle_left_down((0, 0), PointerModifiers::default());
        state.handle_left_up((10, 10));
        assert_eq!(state.history().undo_len(), 1);

        let undo_command = state.handle_key_event(KeyEvent {
            key: KeyCode::U,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(undo_command, Some(InputCommand::Undo));
        assert_eq!(state.history().undo_len(), 0);
        assert_eq!(state.history().redo_len(), 1);

        let redo_command = state.handle_key_event(KeyEvent {
            key: KeyCode::R,
            modifiers: KeyModifiers {
                ctrl: true,
                shift: false,
            },
        });
        assert_eq!(redo_command, Some(InputCommand::Redo));
        assert_eq!(state.history().undo_len(), 1);
        assert_eq!(state.history().redo_len(), 0);
    }

    #[test]
    fn bridge_key_event_routes_exit_command_into_runtime_exit_path() {
        let rt = crate::draw::runtime();
        rt.reset_for_test();
        rt.force_lifecycle_for_test(crate::draw::state::DrawLifecycle::Active);

        let mut state = draw_state(Tool::Pen);
        bridge_key_event_to_runtime(
            &mut state,
            KeyEvent {
                key: KeyCode::Escape,
                modifiers: KeyModifiers::default(),
            },
        );

        assert_eq!(rt.lifecycle(), crate::draw::state::DrawLifecycle::Exiting);
        assert_eq!(
            rt.exit_prompt_state().map(|prompt| prompt.reason),
            Some(ExitReason::UserRequest)
        );
        rt.reset_for_test();
    }

    #[test]
    fn bridge_left_down_routes_shift_and_ctrl_failsafe_exit_commands_into_runtime() {
        let rt = crate::draw::runtime();
        for modifiers in [
            PointerModifiers {
                shift: true,
                ctrl: false,
            },
            PointerModifiers {
                shift: false,
                ctrl: true,
            },
        ] {
            rt.reset_for_test();
            rt.force_lifecycle_for_test(crate::draw::state::DrawLifecycle::Active);

            let mut state = draw_state(Tool::Pen);
            bridge_left_down_to_runtime(&mut state, (20, 20), modifiers);

            assert_eq!(rt.lifecycle(), crate::draw::state::DrawLifecycle::Exiting);
            assert_eq!(
                rt.exit_prompt_state().map(|prompt| prompt.reason),
                Some(ExitReason::UserRequest)
            );
        }
        rt.reset_for_test();
    }
    #[test]
    fn cancel_gestures_route_request_exit_via_same_runtime_path() {
        let mut state = draw_state(Tool::Pen);
        let mut reasons = Vec::new();

        route_command(
            state.handle_key_event(KeyEvent {
                key: KeyCode::Escape,
                modifiers: KeyModifiers::default(),
            }),
            ExitReason::UserRequest,
            |reason| reasons.push(reason),
        );

        route_command(
            state.handle_left_down(
                (10, 10),
                PointerModifiers {
                    shift: true,
                    ctrl: false,
                },
            ),
            ExitReason::UserRequest,
            |reason| reasons.push(reason),
        );

        route_command(
            state.handle_left_down(
                (10, 10),
                PointerModifiers {
                    shift: false,
                    ctrl: true,
                },
            ),
            ExitReason::UserRequest,
            |reason| reasons.push(reason),
        );

        assert_eq!(
            reasons,
            vec![
                ExitReason::UserRequest,
                ExitReason::UserRequest,
                ExitReason::UserRequest,
            ]
        );
    }
}
