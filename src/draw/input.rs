use crate::draw::history::DrawHistory;
use crate::draw::keyboard_hook::{should_consume_key_event, KeyCode, KeyEvent, KeyModifiers};
use crate::draw::model::{DrawObject, Geometry, ObjectStyle, Tool};

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
        if matches!(event.key, KeyCode::Escape) {
            return Some(InputCommand::RequestExit);
        }

        if !should_consume_key_event(true, event) {
            return None;
        }

        match (event.key, event.modifiers) {
            (KeyCode::U, KeyModifiers { ctrl: false, .. }) => {
                let _ = self.history.undo();
                Some(InputCommand::Undo)
            }
            (KeyCode::R, KeyModifiers { ctrl: true, .. }) => {
                let _ = self.history.redo();
                Some(InputCommand::Redo)
            }
            _ => None,
        }
    }

    pub fn request_exit(&self) -> InputCommand {
        InputCommand::RequestExit
    }
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
}
